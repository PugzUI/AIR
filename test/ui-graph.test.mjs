import test from "node:test";
import assert from "node:assert/strict";
import { buildGraph, GRAPH_CARD_SIZE, GRAPH_PADDING } from "../src/graph.mjs";
import { lightfieldGeometry, paintTopology, relatedTopologyIds } from "../ui/topology-fx.mjs";

const host = (id, agents = [], status = "connected") => ({
  id, label: id.toUpperCase(), state: { status }, agents,
});
const session = (hostId, resource, provider, title = resource) => ({
  hostId, hostLabel: hostId.toUpperCase(),
  summary: { resource, provider, title, workingDirectories: ["file:///work/repo"], status: 8 },
});

test("unadvertised provider still renders actual subscribed sessions and status", () => {
  const result = buildGraph([
    host("a", [{ provider: "codex", displayName: "Codex" }]),
    host("b", [], "reconnecting"),
  ], [
    session("a", "codex:/one", "codex", "Review"),
    session("a", "claude:/two", "claude", "Refactor"),
    session("b", "claude:/two", "claude", "Other"),
  ]);
  assert.equal(result.shown, 3);
  assert.equal(result.nodes.filter(({ kind }) => kind === "session").length, 3);
  assert.equal(result.edges.filter(({ chat, sessionUri }) => sessionUri).length, 3);
  const unadvertised = result.nodes.find(({ id }) => id === "a\0agent\0claude");
  assert.equal(unadvertised.advertised, false);
  assert.match(unadvertised.subtitle, /not advertised/);
  assert.equal(result.nodes.find((node) => node.kind === "session" && node.hostId === "b").subtitle, "offline");
  assert.ok(result.edges.filter((edge) => edge.hostId === "b").every((edge) => edge.stale));
  assert.equal(new Set(result.nodes.map(({ id }) => id)).size, result.nodes.length);
});

test("search and caps never create fake sessions or hide matching provider identities", () => {
  const hosts = [host("a", [{ provider: "codex", displayName: "Codex" }])];
  const rows = Array.from({ length: 30 }, (_, index) =>
    session("a", "codex:/s" + index, "codex", "Review " + index));
  const capped = buildGraph(hosts, rows, "", 16);
  assert.equal(capped.shown, 16);
  assert.equal(capped.total, 30);
  assert.equal(capped.nodes.filter((node) => node.kind === "session").length, 16);
  assert.equal(buildGraph(hosts, rows, "a codex repo", 4).shown, 4);
  assert.equal(buildGraph(hosts, [], "codex").nodes.filter((node) => node.kind === "provider").length, 1);
  assert.equal(buildGraph(hosts, rows, "no match").nodes.length, 0);
});

test("capped topology keeps attention and working sessions reachable", () => {
  const hosts = [host("a", [{ provider: "codex", displayName: "Codex" }])];
  const rows = Array.from({ length: 17 }, (_, index) =>
    session("a", "codex:/" + (index === 0 ? "idle" : "background-" + index), "codex",
      index === 0 ? "Idle" : "Background " + index));
  rows.forEach((row) => { row.summary.status = 1; });
  rows[15] = session("a", "codex:/working", "codex", "Working");
  rows[16] = session("a", "codex:/attention", "codex", "Needs input");
  rows[15].summary.status = 8;
  rows[16].summary.status = 24;
  const result = buildGraph(hosts, rows, "", 2);
  assert.deepEqual(result.nodes.filter(({ kind }) => kind === "session").map(({ uri }) => uri), [
    "codex:/working", "codex:/attention",
  ]);
  assert.equal(result.total, 17);
});

test("provider children form a balanced three-column cluster", () => {
  const result = buildGraph(
    [host("a", [{ provider: "codex", displayName: "Codex" }])],
    Array.from({ length: 8 }, (_, index) => session("a", "codex:/s" + index, "codex")),
  );
  const provider = result.nodes.find((node) => node.kind === "provider");
  const sessions = result.nodes.filter((node) => node.kind === "session");
  assert.ok(sessions[0].y + sessions[0].h < provider.y);
  assert.deepEqual(sessions.slice(0, 3).map(({ x }) => x), [
    GRAPH_PADDING.x + 89,
    GRAPH_PADDING.x + 175,
    GRAPH_PADDING.x + 261,
  ]);
  assert.equal(sessions[3].y - sessions[0].y, GRAPH_CARD_SIZE.h + 12);
  assert.ok(provider.x >= 0 && provider.x + provider.w <= result.width);
  assert.ok(sessions.every((node) => node.x + node.w < result.width));
  assert.ok(sessions.every((node) => node.y + node.h <= provider.y));
  assert.ok(result.edges.every((edge) => /^M \d+ \d+ V \d+ H \d+ V \d+$/.test(edge.d)));
  assert.deepEqual(new Set(result.nodes.map(({ kind }) => kind)), new Set(["host", "provider", "session"]));
  assert.ok(result.nodes.every(({ w, h }) =>
    w === GRAPH_CARD_SIZE.w && h === GRAPH_CARD_SIZE.h));
});

test("graph padding leaves a deterministic initial frame around the first host", () => {
  const result = buildGraph(
    [host("a", [{ provider: "codex", displayName: "Codex" }])],
    [session("a", "codex:/one", "codex")],
  );
  const firstHost = result.nodes.find(({ kind }) => kind === "host");
  assert.deepEqual(result.padding, GRAPH_PADDING);
  assert.deepEqual(result.initialScroll, { x: GRAPH_PADDING.x - 48, y: GRAPH_PADDING.y - 48 });
  assert.ok(firstHost.x > result.initialScroll.x);
  assert.ok(firstHost.y > result.initialScroll.y);
  assert.ok(result.width > 1280);
  assert.ok(result.height > 750);
  assert.ok(firstHost.x + firstHost.w < result.width - GRAPH_PADDING.x + 1);
  assert.ok(firstHost.y + firstHost.h < result.height - GRAPH_PADDING.y + 1);
});

test("multi-host provider rows and sessions remain in bounds without overlap", () => {
  const agents = ["codex", "claude", "gemini"].map((provider) => ({ provider, displayName: provider }));
  const hosts = [host("a", agents), host("b", agents)];
  const rows = hosts.flatMap(({ id }) => agents.flatMap(({ provider }, index) =>
    Array.from({ length: index + 1 }, (_, n) => session(id, `${provider}:/${n}`, provider))));
  const graph = buildGraph(hosts, rows, "", rows.length);

  assert.equal(graph.shown, rows.length);
  const hostNodes = graph.nodes.filter(({ kind }) => kind === "host");
  const providers = graph.nodes.filter(({ kind, hostId }) => kind === "provider" && hostId === "a");
  const firstSessions = graph.nodes.filter(({ kind, hostId, provider }) =>
    kind === "session" && hostId === "a" && provider === "claude");
  assert.ok(hostNodes[0].y > GRAPH_PADDING.y);
  assert.equal(hostNodes[1].x, hostNodes[0].x);
  assert.ok(hostNodes[1].y > hostNodes[0].y);
  assert.ok(providers[0].y < hostNodes[0].y);
  assert.equal(providers[1].y, providers[0].y);
  assert.equal(providers[2].y, providers[0].y);
  assert.equal(providers[1].x - providers[0].x, 246 + 24);
  assert.equal(providers[2].x - providers[1].x, 246 + 24);
  assert.equal(firstSessions[1].y - firstSessions[0].y, 0);
  assert.equal(firstSessions[1].x - firstSessions[0].x, GRAPH_CARD_SIZE.w + 12);
  assert.ok(firstSessions.every((node) => node.y + node.h <= providers[1].y));
  const secondSessions = graph.nodes.filter(({ kind, hostId }) =>
    kind === "session" && hostId === "b");
  const secondProviders = graph.nodes.filter(({ kind, hostId }) =>
    kind === "provider" && hostId === "b");
  assert.ok(secondProviders.every((node) => node.y > hostNodes[1].y));
  assert.ok(secondSessions.every((node) => node.y > secondProviders[0].y));
  assert.ok(graph.nodes.every(({ x, y, w, h }) =>
    w === GRAPH_CARD_SIZE.w && h === GRAPH_CARD_SIZE.h && x >= 0 && y >= 0 &&
    x + w <= graph.width && y + h <= graph.height));
  for (const [index, a] of graph.nodes.entries()) {
    for (const b of graph.nodes.slice(index + 1)) {
      assert.ok(a.x + a.w <= b.x || b.x + b.w <= a.x ||
        a.y + a.h <= b.y || b.y + b.h <= a.y, `${a.id} overlaps ${b.id}`);
    }
  }
});

test("host-provider rails attach to facing sides and preserve row alignment", () => {
  const agents = ["codex", "claude", "gemini", "cursor", "zed"]
    .map((provider) => ({ provider, displayName: provider }));
  const graph = buildGraph([host("a", agents)], []);
  const hostNode = graph.nodes.find(({ kind }) => kind === "host");
  const providers = graph.nodes
    .filter(({ kind }) => kind === "provider")
    .sort((a, b) => a.x - b.x);
  const hostEdges = graph.edges.filter(({ from }) => from === hostNode.id);

  const edgeFor = (provider) => hostEdges.find(({ to }) => to === provider.id);
  assert.ok(hostEdges.every(({ d }) =>
    /^M \d+ \d+ (?:V \d+ H \d+ V \d+|H \d+ V \d+ H \d+ V \d+)$/.test(d)));
  for (const provider of providers) {
    const values = edgeFor(provider).d.match(/-?\d+(?:\.\d+)?/g).map(Number);
    assert.equal(values.at(-1), provider.y + provider.h / 2);
  }
  assert.ok(providers.every((provider) => provider.y < hostNode.y));

  const rowOne = providers.filter(({ y }) => y === providers[0].y);
  const rowTwo = providers.filter(({ y }) => y !== providers[0].y);
  assert.equal(rowTwo[1].y, rowTwo[0].y);
  assert.notEqual(rowTwo[0].y, rowOne[0].y);
  const branchEdge = hostEdges.find(({ d }) => d.match(/H \d+ V \d+ H \d+ V \d+$/));
  assert.ok(branchEdge);
  const branchValues = branchEdge.d.match(/-?\d+(?:\.\d+)?/g).map(Number);
  const lighting = lightfieldGeometry(graph, null, 0, graph.height);
  assert.ok(lighting.segments.some(([x1, y1, x2, y2]) =>
    x1 === branchValues[2] && x2 === branchValues[2] &&
    y1 === branchValues[1] && y2 === branchValues[3]));

  const withSessions = buildGraph([host("a", agents)],
    agents.map(({ provider }) => session("a", provider + ":/one", provider)));
  const provider = withSessions.nodes.find(({ kind }) => kind === "provider");
  const child = withSessions.nodes.find(({ kind }) => kind === "session");
  const childEdge = withSessions.edges.find(({ sessionUri }) => sessionUri === child.uri);
  const childValues = childEdge.d.match(/-?\d+(?:\.\d+)?/g).map(Number);
  assert.equal(childValues[1], provider.y);
  assert.equal(childValues.at(-1), child.y + child.h);
});

test("lightfield shader keeps network signals neutral and selection restrained", () => {
  const sources = [];
  const gl = {
    createShader: () => ({}),
    shaderSource: (_shader, source) => sources.push(source),
    compileShader: () => {},
    getShaderParameter: () => false,
    deleteShader: () => {},
  };
  const previousMatchMedia = globalThis.matchMedia;
  globalThis.matchMedia = () => ({ matches: false });
  try {
    paintTopology({ getContext: () => gl }, buildGraph([], []));
  } finally {
    if (previousMatchMedia === undefined) delete globalThis.matchMedia;
    else globalThis.matchMedia = previousMatchMedia;
  }
  assert.match(sources[1], /vec3 signalLight = vec3\(0\.82, 0\.86, 0\.88\)/);
  assert.match(sources[1], /col \+= signalLight \* segGlow/);
  assert.match(sources[1], /col \+= signalLight \* neonFalloff\(dPulse/);
  assert.match(sources[1], /vec3 selectionLime/);
  assert.doesNotMatch(sources[1], /tealBoard|limeNeon|goldData|agentCyan|u_entities/);
});

test("focused lightfield keeps only connectors related to the focused node", () => {
  const graph = buildGraph(
    [host("a", [{ provider: "codex", displayName: "Codex" }])],
    [session("a", "codex:/one", "codex"), session("a", "codex:/two", "codex")],
  );
  const sessionNode = graph.nodes.find(({ kind }) => kind === "session");
  const focused = lightfieldGeometry(graph, null, 0, graph.height, sessionNode.id);
  const all = lightfieldGeometry(graph, null, 0, graph.height);
  assert.ok(focused.segments.length > 0);
  assert.ok(focused.segments.length < all.segments.length);
});

test("session focus keeps host and provider ancestry in the focus island", () => {
  const graph = buildGraph(
    [host("a", [{ provider: "codex", displayName: "Codex" }])],
    [session("a", "codex:/one", "codex"), session("a", "codex:/two", "codex")],
  );
  const sessionNode = graph.nodes.find(({ kind }) => kind === "session");
  const providerNode = graph.nodes.find(({ kind }) => kind === "provider");
  const hostNode = graph.nodes.find(({ kind }) => kind === "host");
  const related = relatedTopologyIds(graph, sessionNode.id);
  assert.ok(related.has(sessionNode.id));
  assert.ok(related.has(providerNode.id));
  assert.ok(related.has(hostNode.id));
  assert.equal(related.size, 3);
});

test("large topology keeps compact top-down geometry and bounded lightfield coverage", () => {
  const rows = Array.from({ length: 32 }, (_, index) =>
    session("a", "codex:/s" + index, "codex", "Session " + index));
  const graph = buildGraph([host("a", [{ provider: "codex", displayName: "Codex" }])],
    rows, "", 32);
  assert.ok(graph.nodes.every(({ x, y, w, h }) =>
    x >= 0 && y >= 0 && x + w <= graph.width && y + h <= graph.height));
  assert.equal(graph.nodes.filter((node) => node.kind === "session").length, 32);
  const selected = { hostId: "a", uri: "codex:/s31" };
  const scrollY = 0;
  const { nodes, segments } = lightfieldGeometry(graph, selected, scrollY, graph.height);
  assert.ok(nodes.length <= 12);
  assert.ok(segments.length <= 32);
  assert.ok(nodes.every((node) => node.y + node.h >= scrollY - 48 &&
    node.y <= scrollY + graph.height + 48));
  assert.ok(nodes.some((node) => node.uri === selected.uri),
    "A visible selected session must receive shader illumination");
  assert.ok(segments.every(([x1, y1, x2, y2]) =>
    [x1, y1, x2, y2].every(Number.isFinite) &&
    Math.max(y1, y2) >= scrollY - 48 &&
    Math.min(y1, y2) <= scrollY + graph.height + 48));
});

test("disconnected hosts keep their native SVG routes but have no live neon conduits", () => {
  const graph = buildGraph(
    [host("a", [{ provider: "codex", displayName: "Codex" }], "reconnecting")],
    [session("a", "codex:/offline", "codex")],
  );
  assert.ok(graph.edges.length > 0);
  assert.ok(graph.edges.every((edge) => edge.stale));
  const lighting = lightfieldGeometry(graph, { hostId: "a", uri: "codex:/offline" },
    0, graph.height);
  assert.equal(lighting.segments.length, 0);
  assert.ok(lighting.nodes.length > 0);
});
