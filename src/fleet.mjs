import { chatReducer, changesetReducer, sessionReducer } from "@microsoft/agent-host-protocol";
import { MultiHostClient } from "@microsoft/agent-host-protocol/hosts";
import { WebSocketTransport } from "@microsoft/agent-host-protocol/ws";

/** @param {any} value */
export function statusOf(value) {
  if (!Number.isInteger(value)) return "unknown";
  if ((value & 24) === 24) return "attention";
  if (value & 2) return "error";
  if (value & 8) return "working";
  if (value & 1) return "idle";
  return "unknown";
}

/** @param {any} input */
export function safeEndpoint(input) {
  const url = new URL(input.trim());
  if (!["ws:", "wss:"].includes(url.protocol) || url.username || url.password ||
      url.hash || url.search || !url.hostname) {
    throw Error("Enter a WebSocket URL without embedded credentials or query parameters");
  }
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if (url.protocol === "ws:" && !local) {
    throw Error("Remote hosts require encrypted wss://");
  }
  return url;
}

/** @param {any} input */
export function workspaceUri(input) {
  const value = input.trim();
  if (!value) return undefined;
  const url = new URL(value);
  if (url.protocol !== "file:" || url.host || url.search || url.hash ||
      !url.pathname || !url.pathname.startsWith("/")) {
    throw Error("Workspace must be an absolute file:/// URI on the selected host");
  }
  return url.href;
}

const MAX_PREVIEW_BYTES = 256 * 1024;

/** @param {any} data */
export function decodeResourceText(data) {
  if (!data || typeof data.data !== "string") throw Error("Host returned no file content");
  if (data.contentType && !/^(text\/|application\/(?:json|xml|javascript|.*\+json|.*\+xml))/.test(data.contentType)) {
    throw Error("The selected file is not text; use an editor for binary files");
  }
  let value;
  if (data.encoding === "utf-8") value = data.data;
  else if (data.encoding === "base64") {
    if (data.data.length > Math.ceil(MAX_PREVIEW_BYTES / 3) * 4 + 8) {
      throw Error("File preview exceeds 256 KiB; inspect it in an editor");
    }
    const bytes = Uint8Array.from(atob(data.data), (character) => character.charCodeAt(0));
    value = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } else throw Error("Unsupported file encoding from host");
  if (new TextEncoder().encode(value).byteLength > MAX_PREVIEW_BYTES) {
    throw Error("File preview exceeds 256 KiB; inspect it in an editor");
  }
  if (value.includes("\0")) throw Error("The selected file appears to be binary");
  return value;
}

/** @param {any} value

 * @param {any} online */
export function sessionState(value, online = true) {
  if (!online) return "offline";
  if (value?.lifecycle === "failed") return "error";
  if (value?.lifecycle === "creating") return "connecting";
  return statusOf(value?.status);
}

/** @param {any} session */
export function attentionFor(session) {
  return (session?.inputNeeded || []).filter(/** @param {any} entry */ (entry) =>
    entry.kind === "toolConfirmation" || entry.kind === "chatInput" ||
    entry.kind === "toolAuthentication");
}

// Only the RFC 6570 variables defined by AHP 1.0 can be expanded: values
// come from real completed turns on the subscribed chat, never guessed IDs.
/** @param {any} entries
 * @param {any[]} [completedTurns]
 * @param {Record<string, any>} [params] */
export function expandChangesets(entries, completedTurns = [], params = {}) {
  const ids = [...new Set(completedTurns.map((turn) => turn?.id)
    .filter((id) => typeof id === "string" && id.length > 0))];
  const pick = /** @param {any} name
 * @param {any} fallback */
               (name, fallback) => {
    const explicit = params[name];
    return explicit ? (ids.includes(explicit) ? explicit : null) : fallback;
  };
  /** @type {Record<string, any>} */
  const values = {
    turnId: pick("turnId", ids.at(-1)),
    originalTurnId: pick("originalTurnId", ids.at(-2)),
    modifiedTurnId: pick("modifiedTurnId", ids.at(-1)),
  };
  const seen = new Set();
  const result = [];
  for (const entry of entries) {
    const template = entry?.uriTemplate;
    if (typeof template !== "string") continue;
    const vars = [...template.matchAll(/\{([^{}]+)\}/g)].map((match) => match[1]);
    const shape = [...new Set(vars)].sort().join(",");
    if (shape !== "" && shape !== "turnId" &&
        shape !== "modifiedTurnId,originalTurnId") continue;
    if (vars.some((name) => !values[name]) ||
        /[{}]/.test(template.replace(/\{[^{}]+\}/g, ""))) continue;
    const uri = template.replace(/\{([^{}]+)\}/g, (_, name) =>
      encodeURIComponent(values[name]));
    if (seen.has(uri)) continue;
    seen.add(uri);
    result.push({ ...entry, uriTemplate: uri, sourceTemplate: template });
  }
  return result;
}

/** @param {any} request

 * @param {any} answers */
export function validateInputAnswers(request, answers) {
  const questions = request.questions || [];
  const known = new Map(questions.map(/** @param {any} question */ (question) => [question.id, question]));
  for (const [id, answer] of Object.entries(answers)) {
    const question = known.get(id);
    if (!question) throw Error("Unknown input question: " + id);
    if (answer?.state === "skipped") {
      if (question.required) throw Error("Complete " + (question.title || question.message));
      continue;
    }
    if (answer?.state !== "submitted") throw Error("Input answer is not submitted: " + id);
    const value = answer.value;
    if (!value || typeof value !== "object") throw Error("Invalid input answer: " + id);
    const invalid = () => { throw Error("Invalid answer for " + (question.title || question.message)); };
    switch (question.kind) {
      case "text":
        if (value.kind !== "text" || typeof value.value !== "string" ||
            (question.min != null && value.value.length < question.min) ||
            (question.max != null && value.value.length > question.max)) invalid();
        break;
      case "number":
      case "integer":
        if (value.kind !== "number" || !Number.isFinite(value.value) ||
            (question.kind === "integer" && !Number.isInteger(value.value)) ||
            (question.min != null && value.value < question.min) ||
            (question.max != null && value.value > question.max)) invalid();
        break;
      case "boolean":
        if (value.kind !== "boolean" || typeof value.value !== "boolean") invalid();
        break;
      case "single-select":
      case "multi-select": {
        const multiple = question.kind === "multi-select";
        if (value.kind !== (multiple ? "selected-many" : "selected") ||
            (multiple ? !Array.isArray(value.value) : typeof value.value !== "string")) invalid();
        const selected = multiple ? value.value :
          (value.value === "" && question.allowFreeformInput ? [] : [value.value]);
        const freeform = value.freeformValues || [];
        if (!Array.isArray(freeform) ||
            (!question.allowFreeformInput && freeform.length > 0) ||
            !freeform.every((entry) => typeof entry === "string" && entry.trim()) ||
            !selected.every(/** @param {any} choice */ (choice) => typeof choice === "string" &&
              question.options.some(/** @param {any} item */ (item) => item.id === choice)) ||
            (question.required && selected.length === 0 && freeform.length === 0) ||
            (multiple && ((question.min != null && selected.length + freeform.length < question.min) ||
              (question.max != null && selected.length + freeform.length > question.max) ||
              new Set(selected).size !== selected.length ||
              new Set(freeform).size !== freeform.length))) invalid();
        break;
      }
      default:
        throw Error("Unsupported input question kind: " + question.kind);
    }
  }
  for (const question of questions) {
    if (question.required && (answers[question.id] || request.answers?.[question.id])?.state !== "submitted") {
      throw Error("Complete " + (question.title || question.message));
    }
  }
}

export class Fleet {
  /** @param {any} onChange
   * @param {MultiHostClient} [multi] */
  constructor(onChange, multi = new MultiHostClient()) {
    this.multi = multi;
    this.onChange = onChange;
    /** @type {{hostId: string, uri: string} | null} */
    this.selected = null;
    /** @type {Record<string, any>} */
    this.slots = { session: null, chat: null, changes: null };
    this.waiting = new Map();
    // AHP workspace trust is deliberately scoped to this browser tab and each
    // host connection. Never infer it from the workspace path or persist it.
    this.trusted = new Map();
    this.changesetParams = {};
    this.lastSignal = null;
    this.error = "";
    // The SDK may still report a live transport if our event consumer fails.
    // Fail closed: without authoritative event updates, mutations are unsafe.
    this.eventStreamHealthy = true;
    this.sequence = 0;
    // Serialize mutations per host, not globally: a stalled host A must never
    // hold up switching to healthy host B. Within one host, unsubscribe cannot
    // race ahead of a subsequent subscribe to the same channel.
    this.subscriptionBarriers = new Map();
    this.closed = false;
    void this.listen();
  }

  /** @param {any} hostId

   * @param {any} operation */
  serializeSubscription(hostId, operation) {
    const running = (this.subscriptionBarriers.get(hostId) || Promise.resolve()).then(operation);
    const settled = running.catch(() => {});
    this.subscriptionBarriers.set(hostId, settled);
    void settled.then(() => {
      if (this.subscriptionBarriers.get(hostId) === settled) this.subscriptionBarriers.delete(hostId);
    });
    return running;
  }

  /** @param {any} slots */
  releaseSubscriptions(slots) {
    return Promise.all(slots.map(/** @param {any} arg0 */ ({ hostId, uri }) =>
      this.serializeSubscription(hostId, () =>
        Promise.resolve().then(() => this.multi.unsubscribe(hostId, uri)).catch(() => {}))));
  }

  changed() {
    this.onChange?.();
  }

  hosts() {
    return this.multi.hostsSnapshot();
  }

  sessions() {
    const items = this.multi.aggregatedSessions();
    if (!this.eventStreamHealthy) return items;
    const selected = this.selected;
    const detail = this.session;
    if (!selected || !detail) return items;
    const host = this.multi.host(selected.hostId);
    if (host?.state.status !== "connected") return items;
    const index = items.findIndex((item) => item.hostId === selected.hostId &&
      item.summary.resource === selected.uri);
    // A freshly created session can be present on its subscribed channel before
    // the host includes it in listSessions. Project that authoritative channel,
    // never a fabricated local session record, into the currently selected view.
    const summary = index >= 0 ? items[index].summary : {
      resource: selected.uri,
      provider: detail.provider,
      title: detail.title,
      status: detail.status,
      workingDirectories: detail.workingDirectories,
      defaultChat: detail.defaultChat,
      chats: detail.chats,
    };
    const liveChat = detail.chats?.length === 1 ? this.chat : null;
    const projected = {
      ...summary,
      lifecycle: detail.lifecycle,
      creationError: detail.creationError,
      ...(liveChat ? {
        status: liveChat.status,
        activity: liveChat.activity,
        modifiedAt: liveChat.modifiedAt,
      } : {}),
    };
    /** @type {any} */
    const item = { hostId: selected.hostId, hostLabel: host.label, summary: projected };
    if (index >= 0) items[index] = item;
    else items.unshift(item);
    return items;
  }

  agents() {
    return this.multi.aggregatedAgents();
  }

  /** @param {any} hostId

   * @param {any} provider */
  modelsFor(hostId, provider) {
    return this.multi.host(hostId)?.agents.find((agent) =>
      agent.provider === provider)?.models || [];
  }

  get session() {
    return this.slots.session?.ready ? this.slots.session.state : null;
  }

  get chat() {
    if (!this.slots.chat?.ready) return null;
    const chats = this.session?.chats;
    if (Array.isArray(chats) && !chats.some((entry) =>
      entry.resource === this.slots.chat.uri && entry.interactivity !== "hidden")) return null;
    return this.slots.chat.state;
  }

  get chatPromptPolicy() {
    if (!this.eventStreamHealthy) {
      return { allowsInput: false, reason: "AHP event stream unavailable. Reload Relay to resubscribe." };
    }
    const chat = this.chat;
    if (this.session?.lifecycle === "failed") {
      return { allowsInput: false, reason: "Session creation failed on the host" };
    }
    if (this.session?.lifecycle === "creating") {
      return { allowsInput: false, reason: "Session is still starting" };
    }
    if (!chat) return { allowsInput: false, reason: "Chat is not ready" };
    if (!chat.interactivity || chat.interactivity === "full") {
      return { allowsInput: true, reason: "" };
    }
    return {
      allowsInput: false,
      reason: `This chat is ${chat.interactivity}. The host does not accept user prompts here.`,
    };
  }

  get changes() {
    return this.slots.changes?.ready ? this.slots.changes.state : null;
  }

  changesetCatalog() {
    return [...(this.session?.changesets || []), ...(this.chat?.changesets || [])];
  }

  availableChangesets(params = this.changesetParams) {
    return expandChangesets(this.changesetCatalog(), this.chat?.turns || [], params);
  }

  get changeset() {
    return this.availableChangesets().find((entry) =>
      entry.uriTemplate === this.slots.changes?.uri) || null;
  }

  currentHost() {
    return this.selected ? this.multi.host(this.selected.hostId) : null;
  }

  /** @param {any} arg0 */
  async addHost({ label, endpoint, token }) {
    const url = safeEndpoint(endpoint);
    const id = label.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
    if (!id) throw Error("Host name is required");
    const credential = String(token || "").trim();
    if (!credential) throw Error("An AHP host access token is required");
    const existing = this.multi.host(id);
    if (existing?.state.status === "connected") {
      throw Error("Host is already connected. Select it, or remove it to change its endpoint.");
    }
    // A failed/reconnecting host cannot authenticate using newly entered
    // credentials. Replace its connection rather than rejecting the same name.
    if (existing) await this.removeHost(id);
    // The secret lives exclusively in this closure and is never persisted or rendered.
    url.searchParams.set("tkn", credential);
    await this.multi.addHost({
      id,
      label: label.trim(),
      transportFactory: async (_id, signal) =>
        WebSocketTransport.connect(url, /** @type {any} */ ({ signal })),
    });
    this.changed();
  }

  /** @param {any} id */
  async removeHost(id) {
    if (this.selected?.hostId === id) await this.select(null);
    await this.multi.removeHost(id);
    this.trusted.delete(id);
    this.changed();
  }

  /** @param {any} hostId

   * @param {any} directory */
  trustWorkspace(hostId, directory) {
    const uri = workspaceUri(directory || "");
    if (!uri) throw Error("Select a workspace before trusting its project files");
    this.requireHost(hostId);
    const previous = this.trusted.get(hostId) || new Set();
    const next = new Set(previous);
    next.add(uri);
    this.trusted.set(hostId, next);
    this.publishTrust(hostId);
  }

  /** @param {any} hostId */
  publishTrust(hostId) {
    const uris = [...(this.trusted.get(hostId) || [])];
    if (!uris.length) return;
    this.requireHost(hostId);
    // The AHP host interprets workspaceTrust on the root channel per client
    // connection. It discards that grant at disconnect; it is not host policy.
    this.multi.dispatch(hostId, "ahp-root://", /** @type {any} */ ({
      type: "root/configChanged",
      config: { workspaceTrust: { enabled: true, trustedUris: uris } },
    }));
  }

  /** @param {any} id */
  async reconnect(id) {
    await this.multi.reconnectHost(id);
  }

  /** @param {any} value */
  async select(value) {
    const previous = Object.values(this.slots).filter(Boolean);
    const revision = ++this.sequence;
    if (this.waiting.size > 0) {
      this.error = "Queued submission was not confirmed before changing the view. Inspect host state before retrying.";
    }
    this.selected = value;
    this.changesetParams = {};
    this.slots = { session: null, chat: null, changes: null };
    this.waiting.clear();
    this.lastSignal = null;
    this.changed();
    const cleanup = this.releaseSubscriptions(previous);
    if (!value) return cleanup;
    // Own-host cleanup precedes the watch via the per-host queue. Cross-host
    // cleanup may finish later and cannot delay this newly selected host.
    void cleanup.catch(() => {});
    if (revision !== this.sequence) return;
    await this.watch("session", value.hostId, value.uri, revision);
  }

  /** @param {any} kind

   * @param {any} hostId

   * @param {any} uri

   * @param {any} revision */
  async watch(kind, hostId, uri, revision = this.sequence) {
    return this.serializeSubscription(hostId, async () => {
      if (!this.eventStreamHealthy) return;
      if (!uri || revision !== this.sequence || this.selected?.hostId !== hostId) return;
      const old = this.slots[kind];
      if (old?.hostId === hostId && old.uri === uri) return;
      if (old) {
        this.slots[kind] = null;
        if (kind === "chat" && this.waiting.size) {
          this.waiting.clear();
          this.error = "Chat changed before the host confirmed queued submissions. Inspect the original chat before retrying.";
        }
        await this.multi.unsubscribe(old.hostId, old.uri).catch(() => {});
      }
      if (revision !== this.sequence || this.selected?.hostId !== hostId) return;
      /** @type {any} */
      const slot = { hostId, uri, kind, ready: false, state: null, seq: -1, buffer: [] };
      this.slots[kind] = slot;
      this.changed();
      try {
        const result = await this.multi.subscribe(hostId, uri);
        if (revision !== this.sequence || this.slots[kind] !== slot) {
          await this.multi.unsubscribe(hostId, uri).catch(() => {});
          return;
        }
        slot.state = result.snapshot?.state || null;
        slot.seq = result.snapshot?.fromSeq ?? -1;
        slot.ready = true;
        slot.buffer.sort(/** @param {any} a
         * @param {any} b */ (a, b) => a.serverSeq - b.serverSeq);
        for (const event of slot.buffer) this.apply(slot, event);
        slot.buffer.length = 0;
        if (kind === "session") {
          void this.findChat(revision);
          void this.findChanges(revision);
        }
        if (kind === "chat") void this.findChanges(revision);
        this.changed();
      } catch (error) {
        if (revision !== this.sequence || this.slots[kind] !== slot) return;
        slot.error = /** @type {any} */ (error)?.message || "Session data unavailable";
        this.error = slot.error;
        this.changed();
      }
    });
  }

  async findChat(revision = this.sequence) {
    const session = this.session;
    if (session?.chats?.some(/** @param {any} entry */ (entry) => entry.resource === this.slots.chat?.uri &&
        entry.interactivity !== "hidden")) return;
    const summary = this.sessions().find((row) => row.hostId === this.selected?.hostId &&
      row.summary.resource === this.selected?.uri)?.summary;
    const available = (session ? session.chats || [] : summary?.chats || [])
      .filter(/** @param {any} entry */ (entry) => entry.interactivity !== "hidden");
    const preferred = session ? session.defaultChat : summary?.defaultChat;
    const uri = available.some(/** @param {any} entry */ (entry) => entry.resource === preferred)
      ? preferred : available[0]?.resource;
    if (!uri && this.slots.chat) {
      const old = this.slots.chat;
      this.slots.chat = null;
      if (this.waiting.size) {
        this.waiting.clear();
        this.error = "Chat was removed before queued submissions were confirmed. Inspect host state before retrying.";
      }
      await this.serializeSubscription(old.hostId, () =>
        this.multi.unsubscribe(old.hostId, old.uri).catch(() => {}));
      this.changed();
      await this.findChanges(revision);
    }
    if (uri && this.selected) await this.watch("chat", this.selected.hostId, uri, revision);
  }

  /** @param {any} uri */
  async selectChat(uri) {
    if (!this.selected || !this.session?.chats?.some(/** @param {any} chat */ (chat) =>
      chat.resource === uri && chat.interactivity !== "hidden")) {
      throw Error("Chat is not part of the selected session");
    }
    this.changesetParams = {};
    await this.watch("chat", this.selected.hostId, uri);
  }

  async loadOlderTurns() {
    const slot = this.slots.chat;
    const cursor = this.chat?.turnsNextCursor;
    if (!slot?.ready || !cursor) throw Error("No older turns available");
    const client = this.requireHost(slot.hostId);
    await client.request("fetchTurns", { channel: slot.uri, cursor });
    // The host sends chat/turnsLoaded as a channel action, never as a second
    // transcript in the RPC response. A stale page must not be appended here.
  }

  async findChanges(revision = this.sequence) {
    const options = this.availableChangesets();
    const current = this.slots.changes?.uri;
    if (current && options.some((entry) => entry.uriTemplate === current)) return;
    if (!options.length) {
      const old = this.slots.changes;
      if (old) {
        this.slots.changes = null;
        await this.serializeSubscription(old.hostId, () =>
          this.multi.unsubscribe(old.hostId, old.uri).catch(() => {}));
        this.changed();
      }
      return;
    }
    const preferred = options.find((item) => item.changeKind === "session") ||
      options.find((item) => item.changeKind === "uncommitted") || options[0];
    await this.selectChangeset(preferred.uriTemplate, revision);
  }

  /** @param {any} uriTemplate

   * @param {any} revision

   * @param {any} params */
  async selectChangeset(uriTemplate, revision = this.sequence, params = this.changesetParams) {
    if (!this.selected || !this.availableChangesets(params).some((entry) => entry.uriTemplate === uriTemplate)) {
      throw Error("Changeset is not advertised by the selected session");
    }
    this.changesetParams = { ...params };
    await this.watch("changes", this.selected.hostId, uriTemplate, revision);
  }

  /** @param {any} fileId

   * @param {any} reviewed */
  reviewFile(fileId, reviewed) {
    const changeset = this.changeset;
    if (!changeset || !Object.hasOwn(changeset.capabilities || {}, "review") ||
      !this.changes?.files?.some(/** @param {any} file */ (file) => file.id === fileId)) {
      throw Error("Host does not allow reviewing this file");
    }
    this.dispatch(this.slots.changes.uri, {
      type: "changeset/filesReviewChanged", files: [fileId], reviewed: Boolean(reviewed),
    });
  }

  /** @param {any} fileId */
  async readEdit(fileId) {
    const slot = this.slots.changes;
    if (!slot?.ready || !this.changeset) throw Error("Select a host changeset first");
    const file = this.changes?.files?.find(/** @param {any} entry */ (entry) => entry.id === fileId);
    if (!file) throw Error("File is no longer in the current changeset");
    const client = this.requireHost(slot.hostId);
    const read = /** @param {any} side */ async (side) => {
      const ref = file.edit?.[side]?.content;
      if (!ref?.uri) return null;
      if (ref.sizeHint > MAX_PREVIEW_BYTES) {
        throw Error("File preview exceeds 256 KiB; inspect it in an editor");
      }
      const response = await client.request("resourceRead", {
        channel: "ahp-root://", uri: ref.uri, encoding: /** @type {any} */ ("utf-8"),
      });
      return decodeResourceText(response);
    };
    // Authoritative ContentRefs are provided by the host changeset itself.
    // There is no filesystem proxy, arbitrary URI input or persistent diff cache.
    const [before, after] = await Promise.all([read("before"), read("after")]);
    const latest = this.changes?.files?.find(/** @param {any} entry */ (entry) => entry.id === fileId);
    const sameRef = /** @param {any} side */ (side) => {
      const one = file.edit?.[side]?.content;
      const other = latest?.edit?.[side]?.content;
      return one?.uri === other?.uri && one?.nonce === other?.nonce;
    };
    if (this.slots.changes !== slot || !latest || !sameRef("before") || !sameRef("after")) {
      throw Error("File or changeset changed while reading; reopen the file");
    }
    return { fileId, before, after };
  }

  async refreshSelection() {
    if (!this.selected || this.currentHost()?.state.status !== "connected") return;
    // Re-seeding from AHP snapshots is also a recovery point for dropped SDK
    // event-buffer entries while a browser tab was suspended.
    const selection = this.selected;
    await this.select(selection);
  }

  /** @param {any} slot

   * @param {any} envelope */
  apply(slot, envelope) {
    if (envelope.rejectionReason) {
      // Rejected actions do not advance serverSeq. ahpd replies with the
      // *current* sequence, so deduplicating by seq first loses the refusal.
      if (envelope.action?.type === "chat/pendingMessageSet") {
        this.waiting.delete(envelope.action.id);
      }
      this.error = envelope.rejectionReason;
      return;
    }
    if (!slot.state || envelope.serverSeq <= slot.seq) return;
    // Events before a subscribe snapshot are replayed only if newer than fromSeq.
    const activeTurnBefore = slot.kind === "chat" ? slot.state.activeTurn?.id : null;
    slot.state = slot.kind === "session" ? sessionReducer(slot.state, envelope.action)
      : slot.kind === "changes" ? changesetReducer(slot.state, envelope.action)
      : chatReducer(slot.state, envelope.action);
    slot.seq = envelope.serverSeq;
    const kind = envelope.action.type;
    if (kind === "chat/pendingMessageSet") this.waiting.delete(envelope.action.id);
    if (kind === "chat/turnStarted") this.waiting.delete(envelope.action.queuedMessageId);
    if ((kind === "chat/turnComplete" || kind === "chat/turnCancelled") &&
        activeTurnBefore === envelope.action.turnId && !slot.state.activeTurn) {
      const pulse = { uri: slot.uri, at: Date.now(), kind };
      this.lastSignal = pulse;
      setTimeout(() => {
        if (this.lastSignal === pulse) {
          this.lastSignal = null;
          this.changed();
        }
      }, 1400);
    }
    if (kind === "session/chatAdded" || kind === "session/chatRemoved" ||
        kind === "session/chatUpdated" ||
        kind === "session/defaultChatChanged") {
      void this.findChat();
    }
    if (kind === "session/changesetsChanged" || kind === "chat/changesetsChanged" ||
        kind === "chat/turnComplete" || kind === "chat/turnsLoaded") {
      void this.findChanges();
    }
  }

  /** @param {any} reason */
  loseEventStream(reason) {
    if (this.closed || !this.eventStreamHealthy) return;
    this.eventStreamHealthy = false;
    this.error = reason;
    this.waiting.clear();
    this.lastSignal = null;
    this.changed();
  }

  async listen() {
    void (async () => {
      try {
        for await (const event of this.multi.events()) {
          if (event.event.type === "action") {
            for (const slot of Object.values(this.slots)) {
              if (!slot || slot.hostId !== event.hostId || slot.uri !== event.channel) continue;
              if (!slot.ready) slot.buffer.push(event.event.params);
              else this.apply(slot, event.event.params);
            }
            if (event.event.params.rejectionReason) this.error = event.event.params.rejectionReason;
          } else if (event.event.type === "sessionSummaryChanged") {
            if (!this.chat && this.selected?.hostId === event.hostId) void this.findChat();
          } else if (event.event.type === "sessionRemoved" &&
              this.selected?.hostId === event.hostId &&
              this.selected.uri === event.event.params.session) {
            // Host deletion (another browser/Executor) is authoritative. A
            // previously subscribed session must not survive as a live ghost.
            void this.select(null).catch((error) => {
              if (!this.closed) {
                this.error = "Could not clear removed session: " + (error?.message || String(error));
                this.changed();
              }
            });
          }
          this.changed();
        }
        this.loseEventStream("AHP event stream closed unexpectedly. Reload Relay to resubscribe.");
      } catch (error) {
        this.loseEventStream("AHP event stream stopped: " + (/** @type {any} */ (error)?.message || String(error)));
      }
    })();
    try {
      for await (const event of this.multi.hostEvents()) {
        if (event.type === "connected" && this.trusted.has(event.hostId)) {
          try {
            this.publishTrust(event.hostId);
          } catch (error) {
            this.error = "Could not restore workspace trust: " +
              (/** @type {any} */ (error)?.message || String(error));
          }
        }
        if (this.eventStreamHealthy && event.type === "connected" && this.selected?.hostId === event.hostId) {
          const target = this.selected;
          // Re-seed from authoritative snapshots after reconnect; cached state is stale.
          const revision = ++this.sequence;
          const stale = Object.values(this.slots).filter(Boolean);
          this.slots = { session: null, chat: null, changes: null };
          if (this.waiting.size > 0) {
            this.error = "Connection changed before queued submissions were confirmed. Inspect the host session before retrying.";
          }
          this.waiting.clear();
          this.lastSignal = null;
          void (async () => {
            await this.releaseSubscriptions(stale);
            if (this.sequence === revision) {
              await this.watch("session", target.hostId, target.uri, revision);
            }
          })().catch((error) => {
            if (!this.closed && this.sequence === revision) {
              this.error = "Could not restore AHP selection: " + (error?.message || String(error));
              this.changed();
            }
          });
        }
        this.changed();
      }
      this.loseEventStream("AHP connection updates closed unexpectedly. Reload Relay to resubscribe.");
    } catch (error) {
      this.loseEventStream("AHP connection updates stopped: " + (/** @type {any} */ (error)?.message || String(error)));
    }
  }

  /** @param {any} hostId
   * @returns {NonNullable<ReturnType<MultiHostClient["client"]>>} */
  requireHost(hostId) {
    if (!this.eventStreamHealthy) {
      throw Error("AHP event stream unavailable. Reload Relay before performing more actions.");
    }
    const host = this.multi.host(hostId);
    if (host?.state.status !== "connected") throw Error("Host is not connected");
    return /** @type {NonNullable<ReturnType<MultiHostClient["client"]>>} */ (this.multi.client(hostId));
  }

  /** @param {any} arg0 */
  async resolveSessionConfig({ hostId, provider, directory, config = {} }) {
    const client = this.requireHost(hostId);
    if (!this.multi.host(hostId)?.agents.some((agent) => agent.provider === provider)) {
      throw Error("Provider is not advertised by the selected host");
    }
    const where = workspaceUri(directory || "");
    return client.request("resolveSessionConfig", {
      channel: "ahp-root://", provider,
      ...(where ? { workingDirectory: where } : {}),
      config,
    });
  }

  /** @param {any} arg0 */
  async createSession({ hostId, provider, directory, config, trust = false }) {
    const client = this.requireHost(hostId);
    if (!this.multi.host(hostId)?.agents.some((agent) => agent.provider === provider)) {
      throw Error("Provider is not advertised by the selected host");
    }
    const dir = workspaceUri(directory || "");
    if (trust) this.trustWorkspace(hostId, directory);
    // ahpd keeps sessions under the provider-qualified canonical URI. Sending
    // an ahp-session alias works but creates a second URI absent from its root
    // catalog; use the canonical spelling from the outset when URI-safe.
    const scheme = /^[a-z][a-z0-9+.-]*$/i.test(provider) ? provider : "ahp-session";
    const uri = scheme + ":/" + crypto.randomUUID();
    await client.request("createSession", {
      channel: uri,
      provider,
      ...(dir ? { workingDirectories: [dir] } : {}),
      ...(config ? { config } : {}),
    });
    await this.select({ hostId, uri });
    return uri;
  }

  /** @param {any} chat

   * @param {any} action */
  dispatch(chat, action) {
    if (!this.selected) throw Error("Select a session");
    this.requireHost(this.selected.hostId);
    if (chat !== this.slots.chat?.uri && chat !== this.slots.changes?.uri &&
      !this.session?.inputNeeded?.some(/** @param {any} request */ (request) => request.chat === chat)) {
      throw Error("Channel is not owned by the selected session");
    }
    this.multi.dispatch(this.selected.hostId, chat, action);
    this.changed();
  }

  /** @param {any} text

   * @param {any} modelId */
  queueTurn(text, modelId) {
    const policy = this.chatPromptPolicy;
    if (!policy.allowsInput) {
      throw Error(policy.reason || "This chat does not accept user prompts");
    }
    if (!text.trim()) throw Error("A prompt is required");
    const provider = this.session?.provider;
    const chosen = modelId && this.modelsFor(/** @type {{hostId: string}} */ (this.selected).hostId, provider)
      .find((model) => model.id === modelId);
    if (modelId && !chosen) throw Error("Model is not advertised by this session's provider");
    if (chosen?.policyState === "disabled") throw Error("Model is disabled by host policy");
    const id = crypto.randomUUID();
    this.waiting.set(id, text.trim());
    try {
      this.dispatch(this.slots.chat.uri, {
        type: "chat/pendingMessageSet",
        kind: "queued",
        id,
        message: { text: text.trim(), origin: { kind: "user" },
          ...(modelId ? { model: { id: modelId } } : {}) },
      });
    } catch (error) {
      this.waiting.delete(id);
      throw error;
    }
  }

  cancelTurn() {
    if (!this.chat?.activeTurn || !this.slots.chat) throw Error("No active turn");
    // This is a cancellation REQUEST: the required duration is deliberately 0,
    // not invented latency telemetry. The host produces the actual end event.
    this.dispatch(this.slots.chat.uri, {
      type: "chat/turnCancelled",
      turnId: this.chat.activeTurn.id,
      duration: 0,
    });
  }

  /** @param {any} entry

   * @param {any} approved

   * @param {any} selectedOptionId */
  confirmTool(entry, approved, selectedOptionId) {
    if (entry.kind !== "toolConfirmation") throw Error("Not a tool confirmation");
    const current = this.requireAttention(entry);
    const call = current.toolCall;
    // A result confirmation has a separate action without an option ID; the
    // tool may still carry options from its earlier invocation approval.
    if (call.status !== "pending-result-confirmation" && call.options?.length) {
      const option = call.options.find(/** @param {any} item */ (item) => item.id === selectedOptionId);
      if (!option || (option.kind === "approve") !== approved) {
        throw Error("Select an approval option from the host");
      }
    }
    const basic = { turnId: current.turnId, toolCallId: call.toolCallId };
    const action = call.status === "pending-result-confirmation"
      ? { type: "chat/toolCallResultConfirmed", ...basic, approved }
      : approved
        ? { type: "chat/toolCallConfirmed", ...basic, approved: true,
            confirmed: "user-action", ...(selectedOptionId ? { selectedOptionId } : {}) }
        : { type: "chat/toolCallConfirmed", ...basic, approved: false, reason: "denied",
            ...(selectedOptionId ? { selectedOptionId } : {}) };
    this.dispatch(current.chat, action);
  }

  /** @param {any} entry

   * @param {any} response

   * @param {any} answers */
  answerInput(entry, response, answers = {}) {
    if (entry.kind !== "chatInput") throw Error("Not an input request");
    const current = this.requireAttention(entry);
    if (!["accept", "decline", "cancel"].includes(response)) throw Error("Invalid input response");
    if (response === "accept") validateInputAnswers(current.request, answers);
    this.dispatch(current.chat, {
      type: "chat/inputCompleted",
      requestId: current.request.id,
      response,
      ...(response === "accept" ? { answers } : {}),
    });
  }

  /** @param {any} entry */
  requireAttention(entry) {
    const current = this.session?.inputNeeded?.find(/** @param {any} item */ (item) => item.id === entry.id);
    if (!current || current.chat !== entry.chat || current.kind !== entry.kind) {
      throw Error("This request is no longer current");
    }
    if (entry.kind === "toolConfirmation" &&
      (current.turnId !== entry.turnId || current.toolCall.toolCallId !== entry.toolCall.toolCallId ||
       current.toolCall.status !== entry.toolCall.status)) {
      throw Error("Tool confirmation changed; review its current state");
    }
    if (entry.kind === "chatInput" && current.request.id !== entry.request.id) {
      throw Error("Input request changed; review its current state");
    }
    return current;
  }

  async disposeSession() {
    if (!this.selected) throw Error("Select a session");
    const { hostId, uri } = this.selected;
    await this.requireHost(hostId).request("disposeSession", { channel: uri });
    await this.select(null);
  }

  async shutdown() {
    if (this.closed) return;
    this.closed = true;
    await this.multi.shutdown();
  }
}
