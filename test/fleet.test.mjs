import test from "node:test";
import assert from "node:assert/strict";
import { Fleet, statusOf, sessionState, safeEndpoint, workspaceUri, attentionFor, validateInputAnswers, decodeResourceText, expandChangesets } from "../src/fleet.mjs";
import { buildGraph } from "../src/graph.mjs";

test("status bits use server activity, without inventing readiness", () => {
  assert.equal(statusOf(1), "idle");
  assert.equal(statusOf(33), "idle");
  assert.equal(statusOf(8), "working");
  assert.equal(statusOf(24), "attention");
  assert.equal(statusOf(2), "error");
  assert.equal(statusOf(null), "unknown");
  assert.equal(sessionState({ status: 8 }, false), "offline");
  assert.equal(sessionState({ status: 1, lifecycle: "failed" }), "error");
  assert.equal(sessionState({ status: 1, lifecycle: "creating" }), "connecting");
});

test("failed session initialization prevents queued prompts", () => {
  const { fleet } = fakeFleet();
  fleet.slots.session.state.lifecycle = "failed";
  fleet.slots.session.state.creationError = { message: "ACP process exited" };
  assert.equal(fleet.chatPromptPolicy.allowsInput, false);
  assert.match(fleet.chatPromptPolicy.reason, /creation failed/);
  assert.throws(() => fleet.queueTurn("must not dispatch"), /creation failed/);
});

test("hidden and removed chats never remain selected from a stale root summary", async () => {
  const { fleet, operations } = fakeFleet();
  fleet.multi.unsubscribe = async (_host, uri) => { operations.push({ unsubscribed: uri }); };
  fleet.multi.subscribe = async (_host, uri) => ({
    snapshot: { fromSeq: 0, state: { status: 1, interactivity: "full", turns: [] } },
  });
  fleet.slots.session.state.chats = [
    { resource: "ahp-chat:/one", interactivity: "hidden" },
    { resource: "ahp-chat:/visible", interactivity: "full" },
  ];
  fleet.slots.session.state.defaultChat = "ahp-chat:/one";
  assert.equal(fleet.chat, null);
  await assert.rejects(fleet.selectChat("ahp-chat:/one"), /not part/);
  await fleet.findChat();
  assert.equal(fleet.slots.chat.uri, "ahp-chat:/visible");
  assert.ok(operations.some((entry) => entry.unsubscribed === "ahp-chat:/one"));
  fleet.slots.session.state.chats = [{ resource: "ahp-chat:/one", interactivity: "hidden" }];
  await fleet.findChat();
  assert.equal(fleet.slots.chat, null);
  assert.equal(fleet.chat, null);
});

test("switching chats before acknowledgment clears pending UI state with an uncertainty warning", async () => {
  const { fleet } = fakeFleet();
  fleet.slots.session.state.chats = [
    { resource: "ahp-chat:/one", interactivity: "full" },
    { resource: "ahp-chat:/two", interactivity: "full" },
  ];
  fleet.multi.unsubscribe = async () => {};
  fleet.multi.subscribe = async () => ({
    snapshot: { fromSeq: 0, state: { status: 1, interactivity: "full", turns: [] } },
  });
  fleet.queueTurn("Pending original chat request");
  assert.equal(fleet.waiting.size, 1);
  await fleet.selectChat("ahp-chat:/two");
  assert.equal(fleet.slots.chat.uri, "ahp-chat:/two");
  assert.equal(fleet.waiting.size, 0);
  assert.match(fleet.error, /before the host confirmed/);
});

test("a failed inbound event consumer closes dispatch despite a connected SDK host", async () => {
  let dispatchCount = 0;
  const host = {
    id: "a", state: { status: "connected" },
    agents: [{ provider: "fixture", displayName: "Fixture" }],
  };
  const multi = {
    host: () => host,
    hostsSnapshot: () => [host],
    aggregatedSessions: () => [],
    events: async function* () { throw Error("inbound event stream failed"); },
    hostEvents: async function* () { await new Promise(() => {}); },
    dispatch: () => { dispatchCount++; },
    client: () => ({ request: async () => {} }),
  };
  const fleet = new Fleet(() => {}, multi);
  fleet.selected = { hostId: "a", uri: "fixture:/one" };
  fleet.slots.session = {
    kind: "session", ready: true, hostId: "a", uri: "fixture:/one",
    state: { status: 1, lifecycle: "ready", provider: "fixture" },
  };
  fleet.slots.chat = {
    kind: "chat", ready: true, hostId: "a", uri: "ahp-chat:/one",
    state: { status: 1, turns: [], interactivity: "full" },
  };
  for (let i = 0; i < 5 && fleet.eventStreamHealthy; i++) await Promise.resolve();
  assert.equal(fleet.eventStreamHealthy, false);
  assert.match(fleet.error, /inbound event stream failed/);
  assert.equal(fleet.chatPromptPolicy.allowsInput, false);
  assert.throws(() => fleet.queueTurn("Unsafe after event stream loss"), /event stream unavailable/);
  assert.equal(dispatchCount, 0);
  assert.equal(fleet.waiting.size, 0);
});

test("an unexpectedly closed AHP event iterator also fails closed", async () => {
  const multi = {
    host: () => ({ id: "a", state: { status: "connected" } }),
    hostsSnapshot: () => [],
    aggregatedSessions: () => [],
    events: async function* () {},
    hostEvents: async function* () { await new Promise(() => {}); },
  };
  const fleet = new Fleet(() => {}, multi);
  for (let i = 0; i < 5 && fleet.eventStreamHealthy; i++) await Promise.resolve();
  assert.equal(fleet.eventStreamHealthy, false);
  assert.match(fleet.error, /closed unexpectedly/);
  assert.throws(() => fleet.requireHost("a"), /event stream unavailable/);
});

test("remote plaintext and credentialed URL entry are refused", () => {
  assert.equal(safeEndpoint("ws://localhost:9187").hostname, "localhost");
  assert.equal(safeEndpoint("wss://fleet.example/ahp").protocol, "wss:");
  for (const value of [
    "ws://fleet.example/ahp", "ws://private-host.example:9187",
    "http://host.example:9187", "wss://user:<credential>@fleet.example",
    "wss://fleet.example/?tkn=<token>", "wss://fleet.example/#<token>",
  ]) assert.throws(() => safeEndpoint(value));
});

test("a missing AHP token cannot create an endlessly reconnecting host", async () => {
  let added = 0;
  const multi = {
    host: () => null,
    addHost: async () => { added++; },
    events: async function* () { await new Promise(() => {}); },
    hostEvents: async function* () { await new Promise(() => {}); },
  };
  const fleet = new Fleet(() => {}, multi);
  await assert.rejects(fleet.addHost({
    label: "example-host", endpoint: "wss://host.example/ahp", token: "  ",
  }), /access token is required/);
  assert.equal(added, 0);
});

test("corrected credentials replace a failed host but never disrupt a connected host", async () => {
  const operations = [];
  let host = { id: "example-host", state: { status: "reconnecting" } };
  const multi = {
    host: () => host,
    removeHost: async () => { operations.push("remove"); host = null; },
    addHost: async ({ id, label }) => {
      operations.push("add");
      host = { id, label, state: { status: "connecting" } };
    },
    events: async function* () { await new Promise(() => {}); },
    hostEvents: async function* () { await new Promise(() => {}); },
  };
  const fleet = new Fleet(() => {}, multi);
  fleet.trusted.set("example-host", new Set(["file:///old/connection"]));
  await fleet.addHost({ label: "example-host", endpoint: "wss://host.example/ahp", token: "replacement-token" });
  assert.deepEqual(operations, ["remove", "add"]);
  assert.equal(fleet.trusted.has("example-host"), false, "Old connection trust must not transfer");
  assert.equal(host.id, "example-host");
  host.state.status = "connected";
  await assert.rejects(fleet.addHost({
    label: "example-host", endpoint: "wss://host.example/ahp", token: "replacement-token-2",
  }), /already connected/);
  assert.deepEqual(operations, ["remove", "add"], "A connected host must not be replaced");
});

test("workspace identifiers are absolute file URIs, not host-relative paths", () => {
  assert.equal(workspaceUri(""), undefined);
  assert.equal(workspaceUri("file:///repo/relay"), "file:///repo/relay");
  for (const value of ["/tmp/x", "../relative", "http://host/x", "file:///tmp/x?y=1", "file://remote/x"])
    assert.throws(() => workspaceUri(value));
});

test("AHP changesets expand only host-defined templates using actual completed turns", () => {
  const catalog = [
    { label: "Uncommitted", uriTemplate: "ahp-changeset:/static" },
    { label: "Turn", uriTemplate: "ahp-changeset:/turn/{turnId}" },
    { label: "Compare", uriTemplate: "ahp-changeset:/compare/{originalTurnId}/{modifiedTurnId}" },
    { label: "Unknown", uriTemplate: "ahp-changeset:/future/{branchId}" },
    { label: "Malformed", uriTemplate: "ahp-changeset:/broken/{turnId" },
  ];
  const turns = [{ id: "turn:one" }, { id: "turn/two" }, { id: "turn:three" }];
  const entries = expandChangesets(catalog, turns);
  assert.deepEqual(entries.map((entry) => entry.uriTemplate), [
    "ahp-changeset:/static",
    "ahp-changeset:/turn/turn%3Athree",
    "ahp-changeset:/compare/turn%2Ftwo/turn%3Athree",
  ]);
  assert.equal(entries[1].sourceTemplate, catalog[1].uriTemplate);
  assert.equal(expandChangesets(catalog, turns, { turnId: "turn:one" })[1].uriTemplate,
    "ahp-changeset:/turn/turn%3Aone");
  assert.deepEqual(expandChangesets(catalog, turns, { turnId: "fabricated" })
    .map((entry) => entry.label), ["Uncommitted", "Compare"]);
  assert.deepEqual(expandChangesets(catalog, [])
    .map((entry) => entry.label), ["Uncommitted"]);
  assert.deepEqual(expandChangesets(catalog, turns.slice(0, 1))
    .map((entry) => entry.label), ["Uncommitted", "Turn"]);
});

test("topology uses host-qualified session identities and actual status", () => {
  const a = { id: "a", label: "Host A", state: { status: "connected" },
    agents: [{ provider: "codex", displayName: "Codex" }] };
  const b = { id: "b", label: "Host B", state: { status: "failed" },
    agents: [{ provider: "codex", displayName: "Codex" }] };
  const rows = [a, b].map((host) => ({
    hostId: host.id, hostLabel: host.label,
    summary: { provider: "codex", resource: "ahp-session:/same-id",
      title: host.id === "a" ? "Alpha audit" : "Bravo audit",
      status: 8, workingDirectories: ["file:///repo"] },
  }));
  const graph = buildGraph([a, b], rows);
  assert.equal(graph.shown, 2);
  assert.equal(new Set(graph.nodes.filter(n => n.kind === "session").map(n => n.id)).size, 2);
  assert.equal(graph.nodes.find(n => n.hostId === "b" && n.kind === "session").subtitle, "offline");
  assert.equal(graph.edges.length, 4);
  assert.equal(buildGraph([a, b], rows, "alpha").shown, 1);
  assert.equal(buildGraph([a, b], rows, "nonexistent").shown, 0);
  assert.equal(buildGraph([a, b], rows, "nonexistent").nodes.length, 0);
  assert.equal(buildGraph([a, b], rows, "", 1).total, 2);
});

function fakeFleet() {
  const operations = [];
  const host = {
    id: "a", label: "A", state: { status: "connected" }, agents: [
      { provider: "fixture", displayName: "Fixture" },
    ],
    sessionSummaries: [],
  };
  const done = async function* () { await new Promise(() => {}); };
  const transport = {
    host: () => host,
    hostsSnapshot: () => [host],
    aggregatedAgents: () => [],
    aggregatedSessions: () => [],
    client: () => ({
      request: async (method, params) => { operations.push({ method, params }); },
    }),
    dispatch: (hostId, channel, action) => {
      operations.push({ hostId, channel, action });
      return { clientSeq: operations.length };
    },
    events: done,
    hostEvents: done,
    shutdown: async () => {},
  };
  const fleet = new Fleet(() => {}, transport);
  fleet.selected = { hostId: "a", uri: "ahp-session:/one" };
  fleet.slots.session = {
    hostId: "a", uri: fleet.selected.uri, kind: "session", ready: true, seq: 0,
    state: { status: 1, inputNeeded: [] },
  };
  fleet.slots.chat = {
    hostId: "a", uri: "ahp-chat:/one", kind: "chat", ready: true, seq: 0,
    state: { status: 1 },
  };
  return { fleet, operations };
}

test("queued turns are addressed to the selected chat and require a host echo", () => {
  const { fleet, operations } = fakeFleet();
  fleet.queueTurn("Check the fixture");
  assert.equal(operations.length, 1);
  assert.equal(operations[0].channel, "ahp-chat:/one");
  assert.equal(operations[0].action.type, "chat/pendingMessageSet");
  assert.equal(operations[0].action.message.origin.kind, "user");
  assert.equal(fleet.waiting.size, 1);
  const id = operations[0].action.id;
  fleet.apply(fleet.slots.chat, {
    serverSeq: 0, action: operations[0].action,
    rejectionReason: "Permission denied",
  });
  assert.equal(fleet.waiting.size, 0);
  assert.equal(fleet.error, "Permission denied");
  assert.equal(fleet.chat.status, 1, "A rejected action must not mutate the chat reducer");
  assert.equal(fleet.slots.chat.seq, 0, "A rejection uses the unchanged host sequence");
  fleet.apply(fleet.slots.chat, {
    serverSeq: 1, action: { type: "chat/activityChanged", activity: "Running" },
  });
  assert.equal(fleet.slots.chat.seq, 1, "The following accepted event must not be dropped");
  assert.equal(fleet.chat.activity, "Running");
  assert.throws(() => fleet.dispatch("ahp-chat:/other", { type: "chat/pendingMessageSet" }),
    /not owned/);
});

test("queued model selection uses only models advertised by the correct host", () => {
  const { fleet, operations } = fakeFleet();
  fleet.multi.host().agents[0].models = [
    { id: "m-fast", provider: "fixture", name: "Fast" },
    { id: "m-locked", provider: "fixture", name: "Locked", policyState: "disabled" },
  ];
  fleet.slots.session.state.provider = "fixture";
  fleet.queueTurn("Use the fast model", "m-fast");
  assert.deepEqual(operations[0].action.message.model, { id: "m-fast" });
  assert.throws(() => fleet.queueTurn("No approval", "m-locked"), /disabled/);
  assert.throws(() => fleet.queueTurn("Unknown", "m-other"), /not advertised/);
  assert.equal(operations.length, 1);
  fleet.slots.chat.state.interactivity = "read-only";
  assert.throws(() => fleet.queueTurn("Should fail"), /does not accept/);
  assert.equal(operations.length, 1);
});

test("chat prompt policy is the single authority for queue submission", () => {
  const { fleet, operations } = fakeFleet();
  assert.deepEqual(fleet.chatPromptPolicy, { allowsInput: true, reason: "" });
  fleet.slots.chat.state.interactivity = "full";
  assert.equal(fleet.chatPromptPolicy.allowsInput, true);
  fleet.slots.chat.state.interactivity = "read-only";
  const policy = fleet.chatPromptPolicy;
  assert.equal(policy.allowsInput, false);
  assert.match(policy.reason, /does not accept/);
  assert.throws(() => fleet.queueTurn("Blocked by policy"), (error) =>
    error.message === policy.reason);
  assert.equal(operations.length, 0);
  fleet.slots.chat.ready = false;
  assert.equal(fleet.chatPromptPolicy.allowsInput, false);
  assert.match(fleet.chatPromptPolicy.reason, /not ready/);
  assert.throws(() => fleet.queueTurn("No chat"), /not ready/);
});

test("session config resolves directly through AHP without inventing defaults", async () => {
  const { fleet, operations } = fakeFleet();
  await fleet.resolveSessionConfig({ hostId: "a", provider: "fixture",
    directory: "file:///tmp/project", config: { isolation: "worktree" } });
  assert.deepEqual(operations[0], {
    method: "resolveSessionConfig",
    params: { channel: "ahp-root://", provider: "fixture",
      workingDirectory: "file:///tmp/project", config: { isolation: "worktree" } },
  });
});

test("older turn pagination uses the host cursor, not a second transcript store", async () => {
  const { fleet, operations } = fakeFleet();
  fleet.slots.chat.state.turnsNextCursor = "opaque-next-page";
  await fleet.loadOlderTurns();
  assert.deepEqual(operations[0], { method: "fetchTurns",
    params: { channel: "ahp-chat:/one", cursor: "opaque-next-page" } });
  delete fleet.slots.chat.state.turnsNextCursor;
  await assert.rejects(fleet.loadOlderTurns(), /No older turns/);
});

test("approval uses exact host-supplied options, never substitutes a different grant", () => {
  const { fleet, operations } = fakeFleet();
  const entry = {
    id: "attention", chat: "ahp-chat:/one", kind: "toolConfirmation",
    turnId: "turn-1", toolCall: {
      toolCallId: "tool-1", status: "pending-confirmation", options: [
        { kind: "approve", id: "once", label: "Allow once" },
        { kind: "deny", id: "reject", label: "Deny" },
      ],
    },
  };
  fleet.slots.session.state.inputNeeded = [entry];
  assert.deepEqual(attentionFor(fleet.session), [entry]);
  assert.throws(() => fleet.confirmTool(entry, true, "reject"), /Select an approval option/);
  assert.throws(() => fleet.confirmTool(entry, true), /Select an approval option/);
  assert.equal(operations.length, 0);
  fleet.confirmTool(entry, false, "reject");
  assert.deepEqual(operations[0].action, {
    type: "chat/toolCallConfirmed", turnId: "turn-1", toolCallId: "tool-1",
    approved: false, reason: "denied", selectedOptionId: "reject",
  });
  fleet.confirmTool(entry, true, "once");
  assert.equal(operations[1].action.selectedOptionId, "once");
  assert.equal(operations[1].action.confirmed, "user-action");
  const resultEntry = { ...entry, toolCall: {
    ...entry.toolCall, status: "pending-result-confirmation",
  } };
  fleet.slots.session.state.inputNeeded = [resultEntry];
  fleet.confirmTool(resultEntry, true);
  assert.deepEqual(operations[2].action, {
    type: "chat/toolCallResultConfirmed", turnId: "turn-1",
    toolCallId: "tool-1", approved: true,
  });
  fleet.slots.session.state.inputNeeded = [];
  assert.throws(() => fleet.confirmTool(entry, true, "once"), /no longer current/);
});

test("input answers require the current authoritative session request", () => {
  const { fleet, operations } = fakeFleet();
  const entry = {
    id: "prompt-1", chat: "ahp-chat:/one", kind: "chatInput",
    request: { id: "ask", questions: [] },
  };
  fleet.slots.session.state.inputNeeded = [entry];
  fleet.answerInput(entry, "decline");
  assert.deepEqual(operations[0].action, {
    type: "chat/inputCompleted", requestId: "ask", response: "decline",
  });
  fleet.slots.session.state.inputNeeded = [];
  assert.throws(() => fleet.answerInput(entry, "accept"), /no longer current/);
});

test("input response validation refuses fabricated option IDs, invalid types and skipped required questions", () => {
  const request = { questions: [
    { id: "reason", kind: "text", required: true, message: "Why?", min: 2, max: 10 },
    { id: "scope", kind: "single-select", required: true, message: "Scope", options: [
      { id: "read", label: "Read" }, { id: "write", label: "Write" },
    ] },
    { id: "count", kind: "integer", message: "Count", min: 1, max: 3 },
  ] };
  const good = {
    reason: { state: "submitted", value: { kind: "text", value: "Review" } },
    scope: { state: "submitted", value: { kind: "selected", value: "read" } },
    count: { state: "submitted", value: { kind: "number", value: 2 } },
  };
  assert.doesNotThrow(() => validateInputAnswers(request, good));
  assert.throws(() => validateInputAnswers(request, { ...good,
    scope: { state: "submitted", value: { kind: "selected", value: "always-allow" } },
  }), /Invalid answer/);
  assert.throws(() => validateInputAnswers(request, { ...good,
    reason: { state: "skipped" },
  }), /Complete Why/);
  assert.throws(() => validateInputAnswers(request, { ...good,
    count: { state: "submitted", value: { kind: "number", value: 1.5 } },
  }), /Invalid answer/);
  assert.throws(() => validateInputAnswers(request, { ...good, unknown: { state: "skipped" } }), /Unknown/);
});

test("freeform input is explicit and never treated as an invented host option", () => {
  const request = { questions: [{ id: "custom", kind: "single-select", required: true,
    message: "Other option", allowFreeformInput: true,
    options: [{ id: "known", label: "Known" }] }] };
  assert.doesNotThrow(() => validateInputAnswers(request, {
    custom: { state: "submitted", value: { kind: "selected", value: "",
      freeformValues: ["legitimate custom answer"] } },
  }));
  assert.throws(() => validateInputAnswers(request, {
    custom: { state: "submitted", value: { kind: "selected", value: "fake",
      freeformValues: ["other"] } },
  }), /Invalid answer/);
  request.questions[0].allowFreeformInput = false;
  assert.throws(() => validateInputAnswers(request, {
    custom: { state: "submitted", value: { kind: "selected", value: "",
      freeformValues: ["not enabled"] } },
  }), /Invalid answer/);
});

test("file review requires a host-advertised review capability and selected file", () => {
  const { fleet, operations } = fakeFleet();
  fleet.slots.session.state.changesets = [{
    label: "Current changes", changeKind: "session", uriTemplate: "fixture:/one/changeset/session",
    capabilities: { review: {} },
  }];
  fleet.slots.changes = { hostId: "a", uri: "fixture:/one/changeset/session",
    kind: "changes", ready: true, state: {
      status: "ready", files: [{ id: "file:///tmp/test.js", edit: {} }],
    },
  };
  assert.equal(fleet.changeset.label, "Current changes");
  fleet.reviewFile("file:///tmp/test.js", true);
  assert.deepEqual(operations[0].action, { type: "changeset/filesReviewChanged",
    files: ["file:///tmp/test.js"], reviewed: true });
  assert.throws(() => fleet.reviewFile("missing", true), /does not allow/);
  fleet.slots.session.state.changesets[0].capabilities = {};
  assert.throws(() => fleet.reviewFile("file:///tmp/test.js", true), /does not allow/);
});

test("file preview decodes only bounded textual content from host resources", async () => {
  assert.equal(decodeResourceText({ data: "hello", encoding: "utf-8", contentType: "text/plain" }), "hello");
  assert.equal(decodeResourceText({ data: btoa("abc"), encoding: "base64",
    contentType: "text/x-javascript" }), "abc");
  assert.throws(() => decodeResourceText({ data: "binary", encoding: "utf-8",
    contentType: "image/png" }), /not text/);
  assert.throws(() => decodeResourceText({ data: "a".repeat(262145), encoding: "utf-8" }), /exceeds 256/);
  const { fleet, operations } = fakeFleet();
  fleet.slots.session.state.changesets = [{ uriTemplate: "fixture:/one/changeset/uncommitted",
    capabilities: {} }];
  fleet.slots.changes = { hostId: "a", uri: "fixture:/one/changeset/uncommitted",
    kind: "changes", ready: true, state: {
      status: "ready", files: [{ id: "file:///tmp/example.js", edit: {
        before: { content: { uri: "file:///tmp/before.js", sizeHint: 40 } },
        after: { content: { uri: "file:///tmp/after.js", sizeHint: 50 } },
      } }],
    },
  };
  fleet.multi.client = () => ({ request: async (method, params) => {
    operations.push({ method, params });
    return { data: params.uri.endsWith("before.js") ? "old" : "new",
      encoding: "utf-8", contentType: "text/plain" };
  } });
  assert.deepEqual(await fleet.readEdit("file:///tmp/example.js"), {
    fileId: "file:///tmp/example.js", before: "old", after: "new",
  });
  assert.deepEqual(operations.map((item) => item.method), ["resourceRead", "resourceRead"]);
  assert.ok(operations.every((item) => item.params.channel === "ahp-root://"));
  await assert.rejects(fleet.readEdit("file:///tmp/unlisted.js"), /no longer/);
});

test("unavailable hosts cannot receive actions regardless of cached state", () => {
  const { fleet, operations } = fakeFleet();
  fleet.multi.host().state.status = "reconnecting";
  assert.throws(() => fleet.queueTurn("Should not send"), /not connected/);
  assert.equal(operations.length, 0);
  assert.equal(fleet.waiting.size, 0, "A rejected local dispatch leaves no phantom queued turn");
});

test("workspace trust is explicit, host-scoped, and sent through native AHP root state", async () => {
  const { fleet, operations } = fakeFleet();
  const a = "file:///repo/relay";
  const b = "file:///repo/another";
  fleet.select = async () => {};
  await fleet.createSession({ hostId: "a", provider: "fixture", directory: a });
  assert.equal(operations.length, 1);
  assert.equal(operations[0].method, "createSession");
  operations.length = 0;
  await fleet.createSession({ hostId: "a", provider: "fixture", directory: a, trust: true });
  assert.equal(operations[0].channel, "ahp-root://");
  assert.equal(operations[0].action.type, "root/configChanged");
  assert.equal(operations[1].method, "createSession");
  operations.length = 0;
  // The host must receive exactly the directories explicitly approved by the operator.
  fleet.trustWorkspace("a", a);
  fleet.trustWorkspace("a", a);
  fleet.trustWorkspace("a", b);
  assert.deepEqual(operations.slice(-1)[0], {
    hostId: "a", channel: "ahp-root://",
    action: { type: "root/configChanged", config: {
      workspaceTrust: { enabled: true, trustedUris: [a, b] },
    } },
  });
  assert.equal(fleet.trusted.get("a").size, 2);
  fleet.multi.host().state.status = "reconnecting";
  assert.throws(() => fleet.trustWorkspace("a", "file:///tmp/unsafe"), /not connected/);
  assert.equal(fleet.trusted.get("a").size, 2);
});

test("created sessions use the same canonical provider URI as ahpd's root catalog", async () => {
  const { fleet, operations } = fakeFleet();
  let selected;
  fleet.select = async (value) => { selected = value; };
  const uri = await fleet.createSession({ hostId: "a", provider: "fixture", directory: "file:///tmp/test" });
  assert.match(uri, /^fixture:\/[0-9a-f-]{36}$/);
  assert.deepEqual(selected, { hostId: "a", uri });
  assert.deepEqual(operations[0], {
    method: "createSession",
    params: { channel: uri, provider: "fixture", workingDirectories: ["file:///tmp/test"] },
  });
});

test("channel switching is serialized and host-qualified even for equal URIs", async () => {
  const operations = [];
  const multi = {
    subscribe: async (hostId, uri) => {
      operations.push(`subscribe ${hostId} ${uri}`);
      return { snapshot: { state: { status: 1, chats: [] }, fromSeq: 5 } };
    },
    unsubscribe: async (hostId, uri) => { operations.push(`unsubscribe ${hostId} ${uri}`); },
    host: (id) => ({ id, label: id, state: { status: "connected" } }),
    aggregatedSessions: () => [],
    events: async function* () { await new Promise(() => {}); },
    hostEvents: async function* () { await new Promise(() => {}); },
  };
  const fleet = new Fleet(() => {}, multi);
  fleet.selected = { hostId: "b", uri: "b:/one" };
  fleet.slots.chat = { kind: "chat", hostId: "a", uri: "ahp-chat:/shared", ready: true, state: {} };
  await fleet.watch("chat", "b", "ahp-chat:/shared");
  assert.deepEqual(operations, [
    "unsubscribe a ahp-chat:/shared",
    "subscribe b ahp-chat:/shared",
  ]);
  assert.equal(fleet.slots.chat.hostId, "b");
});

test("overlapping selections never leave a stale subscription behind", async () => {
  const operations = [];
  let releaseFirst;
  const firstGate = new Promise((resolve) => { releaseFirst = resolve; });
  const multi = {
    subscribe: async (hostId, uri) => {
      operations.push(`subscribe ${uri}`);
      if (uri === "fixture:/first") await firstGate;
      return { snapshot: { state: { status: 1, chats: [] }, fromSeq: 5 } };
    },
    unsubscribe: async (_hostId, uri) => { operations.push(`unsubscribe ${uri}`); },
    host: () => ({ id: "a", label: "a", state: { status: "connected" } }),
    aggregatedSessions: () => [],
    events: async function* () { await new Promise(() => {}); },
    hostEvents: async function* () { await new Promise(() => {}); },
  };
  const fleet = new Fleet(() => {}, multi);
  const first = fleet.select({ hostId: "a", uri: "fixture:/first" });
  for (let i = 0; i < 10 && !operations.length; i++) await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(operations, ["subscribe fixture:/first"]);
  const second = fleet.select({ hostId: "a", uri: "fixture:/second" });
  releaseFirst();
  await Promise.all([first, second]);
  assert.equal(fleet.selected.uri, "fixture:/second");
  assert.equal(fleet.slots.session.uri, "fixture:/second");
  assert.ok(operations.indexOf("unsubscribe fixture:/first") > operations.indexOf("subscribe fixture:/first"));
  assert.ok(operations.indexOf("subscribe fixture:/second") > operations.indexOf("unsubscribe fixture:/first"));
});

test("a stalled host unsubscribe cannot block switching to a healthy host", async () => {
  let releaseOldHost;
  const stalled = new Promise((resolve) => { releaseOldHost = resolve; });
  const operations = [];
  const multi = {
    subscribe: async (hostId, uri) => {
      operations.push("subscribe " + hostId + " " + uri);
      return { snapshot: { state: { status: 1, chats: [] }, fromSeq: 1 } };
    },
    unsubscribe: async (hostId, uri) => {
      operations.push("unsubscribe " + hostId + " " + uri);
      if (hostId === "a") await stalled;
    },
    host: (id) => ({ id, label: id, state: { status: "connected" } }),
    aggregatedSessions: () => [],
    events: async function* () { await new Promise(() => {}); },
    hostEvents: async function* () { await new Promise(() => {}); },
  };
  const fleet = new Fleet(() => {}, multi);
  fleet.selected = { hostId: "a", uri: "fixture:/old" };
  fleet.slots.session = {
    kind: "session", hostId: "a", uri: "fixture:/old", ready: true,
    state: { status: 1, chats: [] },
  };
  try {
    const selectedB = fleet.select({ hostId: "b", uri: "fixture:/new" });
    const finished = await Promise.race([
      selectedB.then(() => true),
      new Promise((resolve) => setTimeout(() => resolve(false), 300)),
    ]);
    assert.equal(finished, true, "A slow host A must not block subscribing to B");
    assert.equal(fleet.slots.session?.hostId, "b");
    assert.ok(operations.includes("subscribe b fixture:/new"));
  } finally {
    releaseOldHost();
  }
});

test("a selected live session missing from the catalog is projected from AHP channel state", () => {
  const { fleet } = fakeFleet();
  fleet.slots.session.state = {
    provider: "fixture", title: "Actual test session", status: 8,
    defaultChat: "ahp-chat:/one", chats: [{ resource: "ahp-chat:/one" }],
    workingDirectories: ["file:///test"],
  };
  fleet.slots.chat.state = {
    status: 1, modifiedAt: "2026-10-23T12:00:00Z",
    turns: [], activeTurn: undefined,
  };
  const projected = fleet.sessions();
  assert.equal(projected.length, 1);
  assert.equal(projected[0].summary.resource, "ahp-session:/one");
  assert.equal(projected[0].summary.title, "Actual test session");
  assert.equal(projected[0].summary.status, 1, "The native chat state supersedes stale session metadata");
  assert.equal(projected[0].summary.modifiedAt, "2026-10-23T12:00:00Z");
  fleet.multi.host().state.status = "reconnecting";
  assert.equal(fleet.sessions().length, 0, "Never report an unavailable session as live");
});
