import { LitElement, html, svg, nothing } from "lit";
import { Fleet, attentionFor, sessionState, statusOf } from "../src/fleet.mjs";
import { buildGraph } from "../src/graph.mjs";
import { edgeGradientCoords, paintTopology, releaseTopology } from "./topology-fx.mjs";

const short = (value, length = 32) =>
  String(value || "—").length > length ? String(value).slice(0, length - 1) + "…" : String(value || "—");
const nodeTitleLines = (node) => {
  const limit = node.kind === "session" ? 10 : 11;
  const words = String(node.title || "—").trim().split(/\s+/).filter(Boolean);
  const lines = [];
  let current = "";
  for (const word of words) {
    if (!current) current = word;
    else if ((current + " " + word).length <= limit) current += " " + word;
    else {
      lines.push(current);
      current = word;
    }
  }
  if (current) lines.push(current);
  const folded = lines.slice(0, 2).map((line) => short(line, limit));
  if (lines.length > 2 && !folded[1].endsWith("…")) {
    folded[1] += "…";
  }
  return folded;
};
const nodeTitleFontSize = (node, lines) => {
  const originalLength = String(node.title || "—").trim().length;
  if (lines.length === 1 && originalLength <= 10) return 9.0;
  if (lines.length === 1 && originalLength <= 20) return 6.5;
  return 6.0;
};
const nodeTitleSizeClass = (fontSize) =>
  fontSize >= 9.0 ? "title-short" : fontSize >= 6.5 ? "title-medium" : "title-long";
const nodeTitleLayout = (node, lines) => {
  const fontSize = nodeTitleFontSize(node, lines);
  const topPad = lines.length > 1 ? 2.2 : (fontSize >= 9.0 ? 3.0 : 4.5);
  const lineStep = lines.length > 1 ? 6.2 : 0;
  const firstBaseline = node.y + topPad + fontSize;
  return { fontSize, firstBaseline, lineStep };
};
const DEMO_BOOTSTRAP_ENDPOINTS = Object.freeze({
  "Demo Primary": "ws://127.0.0.1:42000",
  "Demo Secondary": "ws://127.0.0.1:42001",
});
const demoHostId = (label) => String(label).trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
const interAgentPath = (nodeA, nodeB) => {
  if (!nodeA || !nodeB) return "";
  const w = nodeA.w || 74;
  const h = nodeA.h || 32;
  const ax = Math.round(nodeA.x);
  const ay = Math.round(nodeA.y);
  const bx = Math.round(nodeB.x);
  const by = Math.round(nodeB.y);

  if (ay === by) {
    const x1 = ax < bx ? ax + w : ax;
    const x2 = ax < bx ? bx : bx + w;
    const y = Math.round(ay + h / 2);
    return `M ${x1} ${y} H ${x2}`;
  }
  if (ax === bx) {
    const y1 = ay < by ? ay + h : ay;
    const y2 = ay < by ? by : by + h;
    const x = Math.round(ax + w / 2);
    return `M ${x} ${y1} V ${y2}`;
  }
  const y1 = ay < by ? ay + h : ay;
  const y2 = ay < by ? by : by + h;
  const midY = Math.round((y1 + y2) / 2);
  const x1 = Math.round(ax + w / 2);
  const x2 = Math.round(bx + w / 2);
  return `M ${x1} ${y1} V ${midY} H ${x2} V ${y2}`;
};
const isDemoBootstrap = (payload) => {
  const hosts = payload?.hosts;
  const labels = Object.keys(DEMO_BOOTSTRAP_ENDPOINTS);
  return payload?.schemaVersion === 1 && Array.isArray(hosts) && hosts.length === labels.length &&
    labels.every((label) => hosts.some((entry) => entry?.label === label &&
      entry.endpoint === DEMO_BOOTSTRAP_ENDPOINTS[label]));
};
const nodeStateLabel = (status) => ({
  working: "WORKING",
  attention: "NEEDS INPUT",
  error: "ERROR",
  failed: "ERROR",
  connecting: "CONNECTING",
  reconnecting: "RECONNECTING",
  offline: "OFFLINE",
  disconnected: "OFFLINE",
})[status] || "";
const nodeToolDefs = (node) => node.kind === "session" ? [
  { id: "state", label: "State" },
  { id: "workspace", label: "Workspace" },
  { id: "provider", label: "Provider" },
] : node.kind === "provider" ? [
  { id: "state", label: "State" },
  { id: "sessions", label: "Sessions" },
  { id: "identity", label: "Identity" },
] : [
  { id: "state", label: "State" },
  { id: "agents", label: "Agents" },
  { id: "sessions", label: "Sessions" },
];
const nodeToolGlyph = (id, cx, cy) => {
  if (id === "state") return svg`<circle class="node-tool-icon" cx="${cx}" cy="${cy}" r="2"></circle>`;
  if (id === "workspace") return svg`<path class="node-tool-icon" d="M${cx - 2.25} ${cy - 1.55}h1.5l.75.75h2.25v2.35h-4.5Z"></path>`;
  if (id === "sessions") return svg`<path class="node-tool-icon" d="M${cx - 2.6} ${cy - 2}h5.2M${cx - 2.6} ${cy}h5.2M${cx - 2.6} ${cy + 2}h3.5"></path>`;
  return svg`<polygon class="node-tool-icon" points="${cx},${cy - 2.2} ${cx + 2.2},${cy} ${cx},${cy + 2.2} ${cx - 2.2},${cy}"></polygon>`;
};
const icon = (name) => {
  const paths = {
    plus: svg`<path d="M6 2v8M2 6h8"/>`,
    close: svg`<path d="M2.5 2.5l7 7M9.5 2.5l-7 7"/>`,
    refresh: svg`<path d="M10 6a4 4 0 1 1-1.1-2.75M10 2v2.5H7.5"/>`,
    "arrow-right": svg`<path d="M2 6h8M7 3l3 3-3 3"/>`,
    "arrow-up": svg`<path d="M6 10V2M3 5l3-3 3 3"/>`,
    "arrow-down": svg`<path d="M6 2v8M3 7l3 3 3-3"/>`,
    check: svg`<path d="M2 6.5l2.5 2.5L10 3.5"/>`,
    warning: svg`<path d="M6 2L10.5 10H1.5Z"/>`,
    "chevron-right": svg`<path d="M4 2.5L7.5 6 4 9.5"/>`,
    "chevron-down": svg`<path d="M2.5 4L6 7.5 9.5 4"/>`,
    "chevron-up": svg`<path d="M2.5 8L6 4.5 9.5 8"/>`,
    fit: svg`<path d="M2 4.5V2h2.5M7.5 2H10v2.5M10 7.5V10H7.5M4.5 10H2V7.5"/>`,
  };
  return html`<svg class="icon-svg" viewBox="0 0 12 12" aria-hidden="true">${paths[name] || nothing}</svg>`;
};
const nodeSignalLevel = (status) => ({
  working: 4,
  attention: 4,
  error: 4,
  failed: 4,
  connecting: 3,
  reconnecting: 3,
  connected: 2,
  idle: 1,
  offline: 0,
  disconnected: 0,
})[status] || 0;
const labelText = (value) => typeof value === "string" ? value : value?.markdown || "";
const when = (value) => value && !Number.isNaN(new Date(value).getTime())
  ? new Date(value).toLocaleString() : "Not reported";
const tail = (value) => String(value || "").split("/").filter(Boolean).at(-1) || value || "—";
const safeLink = (value) => {
  try {
    const url = new URL(value);
    return ["https:", "http:"].includes(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
};
const safeError = (value) => String(value || "Unknown error")
  .replace(/([?&]tkn=)[^&#\s"'<>]+/gi, "$1[redacted]");
const changeStats = (diff) => {
  const stats = [];
  if (Number.isInteger(diff?.added)) stats.push("+" + diff.added);
  if (Number.isInteger(diff?.removed)) stats.push("−" + diff.removed);
  return stats.join(" / ");
};
const summaryStats = (summary) => {
  if (!summary) return "Not reported";
  const stats = [];
  if (Number.isInteger(summary.files)) stats.push(summary.files + " files");
  if (Number.isInteger(summary.additions)) stats.push("+" + summary.additions);
  if (Number.isInteger(summary.deletions)) stats.push("−" + summary.deletions);
  return stats.join(" · ") || "Not reported";
};
const fileLabel = (file) => file.edit?.after?.uri || file.edit?.before?.uri || file.id;
const fileKind = (file) => {
  const { before, after } = file.edit || {};
  if (before && !after) return "Deleted";
  if (before && after && before.uri !== after.uri) return "Renamed";
  return "Changed";
};

function readAnswer(question, data) {
  const values = data.getAll(question.id).map(String).filter(Boolean);
  const kind = question.kind;
  const freeform = question.allowFreeformInput &&
    (kind === "single-select" || kind === "multi-select")
      ? String(data.get(question.id + ":freeform") || "").trim() : "";
  if (!values.length && !freeform && question.required)
    throw Error("Complete " + (question.title || question.message));
  if (!values.length && !freeform) return { state: "skipped" };
  let value;
  if (kind === "boolean") value = { kind: "boolean", value: values[0] === "true" };
  else if (kind === "number" || kind === "integer") {
    const number = Number(values[0]);
    if (!Number.isFinite(number) || (kind === "integer" && !Number.isInteger(number)))
      throw Error("Invalid number for " + question.id);
    value = { kind: "number", value: number };
  } else if (kind === "single-select") value = {
    kind: "selected", value: values[0] || "", ...(freeform ? { freeformValues: [freeform] } : {}),
  };
  else if (kind === "multi-select") value = {
    kind: "selected-many", value: values, ...(freeform ? { freeformValues: [freeform] } : {}),
  };
  else value = { kind: "text", value: values[0] };
  return { state: "submitted", value };
}

export class RelayFleet extends LitElement {
  createRenderRoot() { return this; }

  constructor() {
    super();
    this.fleet = new Fleet(() => {
      if (!this.fleet.selected && this.drawerMode === "session") this.drawerMode = "";
      if (this.filePreview && this.filePreview.changesState !== this.fleet.changes) {
        this.filePreview = null;
      }
      this.requestUpdate();
    });
    this.activeHost = "";
    this.filterText = "";
    this.drawerMode = "";
    this.connectOpen = false;
    this.hostPanelOpen = false;
    this.sessionTab = "chat";
    this.catalogOpen = false;
    this.busy = false;
    this.catalogLimit = 100;
    this.focusedNodeId = "";
    this.previewNodeId = "";
    this.nodeViewState = new Map();
    this.modelId = "";
    this.historyOffset = 0;
    this.explicitChatUri = "";
    this.sessionConfig = null;
    this.sessionConfigKey = "";
    this.configRevision = 0;
    this.devBootstrapStarted = false;
    this.demoBootstrapActive = false;
    this.demoHostIds = new Set();
    this.demoSignalFrame = 0;
    this.demoSignalStartedAt = 0;
    this.demoSignalEdge = "";
    this.filePreview = null;
    this.mapScrollFrame = 0;
    this.mapPanX = 0;
    this.mapPanY = 0;
    this.mapZoom = 1;
    this.mapFrameKey = "";
    this.mapPointer = null;
    this.mapDidPan = false;
    this.spaceHeld = false;
    this.leftPanelWidth = 224;
    this.rightDrawerWidth = 224;
    this.panelResize = null;
    this.handleTopologyResize = () => this.scheduleTopologyPaint();
    this.handleMotionChange = () => {
      this.lastTopologyPaintKey = null;
      if (this.motionPreference?.matches) this.stopDemoSignalLoop();
      else this.startDemoSignalLoop();
      this.scheduleTopologyPaint();
    };
    this.handleShortcuts = (event) => {
      const shortcutsOpen = this.querySelector(".shortcuts-dialog")?.open;
      const sideDrawerOpen = this.querySelector(".side-drawer")?.open;
      if (event.key === "Escape" && !shortcutsOpen && !sideDrawerOpen) {
        if (this.connectOpen) {
          event.preventDefault();
          this.closeConnectForm();
          return;
        }
        if (this.hostPanelOpen) {
          event.preventDefault();
          this.closeHostPanel();
          return;
        }
        if (this.previewNodeId) {
          event.preventDefault();
          this.closePreview();
          return;
        }
      }
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey ||
          shortcutsOpen || sideDrawerOpen ||
          event.target.closest?.("input, select, textarea, [contenteditable], [role='textbox']")) return;
      if (event.key === "/") {
        event.preventDefault();
        this.querySelector("#session-search")?.focus();
      } else if (event.key === "?") {
        event.preventDefault();
        this.openShortcuts();
      }
    };
    this.handleVisibility = () => {
      if (document.visibilityState === "visible" && this.fleet.selected &&
          this.fleet.currentHost()?.state.status === "connected" &&
          typeof this.fleet.refreshSelection === "function") {
        this.filePreview = null;
        void this.run(() => this.fleet.refreshSelection());
      }
    };
  }

  connectedCallback() {
    super.connectedCallback();
    document.addEventListener("keydown", this.handleShortcuts);
    document.addEventListener("visibilitychange", this.handleVisibility);
    window.addEventListener("resize", this.handleTopologyResize);
    this.motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
    this.motionPreference.addEventListener("change", this.handleMotionChange);
    void this.bootstrapDevHosts();
    this.startDemoSignalLoop();
  }

  async bootstrapDevHosts() {
    if (this.devBootstrapStarted) return;
    this.devBootstrapStarted = true;
    try {
      const response = await fetch("/dev-hosts.json", { cache: "no-store" });
      if (response.status === 404) return;
      if (typeof EventSource === "function" && !this.devReload) {
        this.devReload = new EventSource("/dev-reload");
        this.devReload.onmessage = (event) => {
          if (event.data === "css") {
            const link = document.querySelector('link[rel="stylesheet"]');
            if (link) link.href = `/style.css?t=${Date.now()}`;
          } else if (event.data === "reload") {
            location.reload();
          }
        };
      }
      if (!response.ok) throw Error("Demo host bootstrap failed");
      const payload = await response.json();
      if (payload?.schemaVersion !== 1 || !Array.isArray(payload.hosts))
        throw Error("Invalid demo host bootstrap");
      for (const entry of payload.hosts) {
        if (!entry || typeof entry.label !== "string" ||
            typeof entry.endpoint !== "string" || typeof entry.token !== "string") continue;
        if (this.fleet.hosts().some((host) => host.label === entry.label)) continue;
        await this.fleet.addHost(entry);
      }
      this.demoBootstrapActive = isDemoBootstrap(payload);
      this.demoHostIds = this.demoBootstrapActive
        ? new Set(payload.hosts.map((entry) => demoHostId(entry.label))) : new Set();
      this.activeHost ||= this.fleet.hosts()[0]?.id || "";
      this.startDemoSignalLoop();
      this.requestUpdate();
    } catch (error) {
      this.fleet.error = safeError(error?.message || error);
      this.requestUpdate();
    }
  }

  updated() {
    const drawer = this.querySelector(".side-drawer");
    if (this.drawerMode && drawer && !drawer.open) drawer.showModal();
    else if (!this.drawerMode && drawer?.open) drawer.close();
    if (this.connectOpen && this.pendingFocus === "connect") {
      this.querySelector('#connect-panel input[name="label"]')?.focus();
      this.pendingFocus = "";
    }
    this.frameMapToGraph();
    this.updateMapTransform();
    this.drawTopology();
  }

  demoPulse() {
    const pulses = this.demoPulses();
    return pulses[0] ? { edge: { d: pulses[0].d }, progress: 0 } : null;
  }

  demoPulses() {
    if (!this.demoBootstrapActive || !this.demoHostIds.size ||
        this.motionPreference?.matches || !this.fleet.eventStreamHealthy ||
        !this.visibleGraph || !this.demoSignalStartedAt) return [];

    const now = Math.max(0, performance.now() - this.demoSignalStartedAt);
    const allEdges = this.visibleGraph.edges.filter((edge) =>
      !edge.stale && this.demoHostIds.has(edge.hostId));
    if (!allEdges.length) return [];

    const nodes = this.visibleGraph.nodes;

    // Edges from providers to sessions (agent delegating to subagent)
    const primarySubagentEdges = allEdges.filter((e) =>
      e.sessionUri && e.hostId === "demo-primary");
    const secondarySubagentEdges = allEdges.filter((e) =>
      e.sessionUri && e.hostId === "demo-secondary");

    // Edges from host to provider (host coordination)
    const hostEdges = allEdges.filter((e) => !e.sessionUri);

    // Inter-agent peer delegation paths on each host
    const interAgentPaths = [];
    for (const hostId of ["demo-primary", "demo-secondary"]) {
      const providers = nodes.filter((n) => n.kind === "provider" && n.hostId === hostId);
      const byName = new Map(providers.map((p) => [p.provider, p]));
      if (hostId === "demo-primary") {
        if (byName.get("planner") && byName.get("builder")) {
          interAgentPaths.push(interAgentPath(byName.get("planner"), byName.get("builder")));
        }
        if (byName.get("builder") && byName.get("reviewer")) {
          interAgentPaths.push(interAgentPath(byName.get("builder"), byName.get("reviewer")));
        }
        if (byName.get("reviewer") && byName.get("qa")) {
          interAgentPaths.push(interAgentPath(byName.get("reviewer"), byName.get("qa")));
        }
        if (byName.get("scout") && byName.get("planner")) {
          interAgentPaths.push(interAgentPath(byName.get("scout"), byName.get("planner")));
        }
      } else {
        if (byName.get("builder") && byName.get("reviewer")) {
          interAgentPaths.push(interAgentPath(byName.get("builder"), byName.get("reviewer")));
        }
        if (byName.get("reviewer") && byName.get("qa")) {
          interAgentPaths.push(interAgentPath(byName.get("reviewer"), byName.get("qa")));
        }
        if (byName.get("qa") && byName.get("watcher")) {
          interAgentPaths.push(interAgentPath(byName.get("qa"), byName.get("watcher")));
        }
      }
    }
    const validInterAgent = interAgentPaths.filter(Boolean);

    // Reverse subagent reporting paths (subagent session -> provider)
    const reverseSubagentPaths = (edges) => edges.map((e) => {
      const m = e.d?.match(/^M\s+([-\d.]+)\s+([-\d.]+)\s+V\s+([-\d.]+)\s+H\s+([-\d.]+)\s+V\s+([-\d.]+)$/);
      if (!m) return null;
      return `M ${m[4]} ${m[5]} V ${m[3]} H ${m[1]} V ${m[2]}`;
    }).filter(Boolean);

    const primaryReverse = reverseSubagentPaths(primarySubagentEdges);
    const secondaryReverse = reverseSubagentPaths(secondarySubagentEdges);

    const slots = [];

    // Slot 0: Primary Agent -> Subagent delegation (cycles through primary agent-session edges)
    if (primarySubagentEdges.length) {
      const idx = Math.floor(now / 2000) % primarySubagentEdges.length;
      slots.push({
        id: "pri-subagent",
        d: primarySubagentEdges[idx].d,
        duration: "0.82s",
        delay: "-0.15s",
      });
    }

    // Slot 1: Secondary Agent -> Subagent delegation (cycles through secondary agent-session edges)
    if (secondarySubagentEdges.length) {
      const idx = Math.floor(now / 2400) % secondarySubagentEdges.length;
      slots.push({
        id: "sec-subagent",
        d: secondarySubagentEdges[idx].d,
        duration: "0.95s",
        delay: "-0.42s",
      });
    }

    // Slot 2: Inter-agent peer delegation (Planner -> Builder, Builder -> Reviewer, Reviewer -> QA, etc.)
    if (validInterAgent.length) {
      const idx = Math.floor(now / 1800) % validInterAgent.length;
      slots.push({
        id: "inter-agent",
        d: validInterAgent[idx],
        duration: "0.76s",
        delay: "-0.65s",
      });
    }

    // Slot 3: Subagent reporting back to agent (reverse path)
    const allReverse = [...primaryReverse, ...secondaryReverse];
    if (allReverse.length) {
      const idx = Math.floor(now / 2200) % allReverse.length;
      slots.push({
        id: "subagent-report",
        d: allReverse[idx],
        duration: "1.05s",
        delay: "-0.28s",
      });
    }

    // Slot 4: Host orchestration / Concurrent secondary subagent delegation
    const mixedPool = [...(hostEdges.length ? hostEdges.map((e) => e.d) : []), ...secondarySubagentEdges.map((e) => e.d)];
    if (mixedPool.length) {
      const idx = Math.floor(now / 2600) % mixedPool.length;
      slots.push({
        id: "orch-delegation",
        d: mixedPool[idx],
        duration: "0.88s",
        delay: "-0.50s",
      });
    }

    return slots;
  }

  startDemoSignalLoop() {
    if (!this.demoBootstrapActive || this.motionPreference?.matches ||
        this.demoSignalFrame || !this.isConnected) return;
    this.demoSignalStartedAt = performance.now();
    this.demoSignalPaths = null;
    const tick = () => {
      this.demoSignalFrame = 0;
      if (!this.isConnected || !this.demoBootstrapActive || this.motionPreference?.matches ||
          !this.fleet.eventStreamHealthy) return;
      const pulses = this.demoPulses();
      if (pulses && pulses.length) {
        if (!this.demoSignalPaths || this.demoSignalPaths.length !== pulses.length) {
          this.demoSignalPaths = Array.from(this.querySelectorAll(".demo-signal"));
        }
        for (let i = 0; i < pulses.length; i++) {
          const pathEl = this.demoSignalPaths[i];
          const pulse = pulses[i];
          if (pathEl && pulse?.d && pathEl.getAttribute("d") !== pulse.d) {
            pathEl.setAttribute("d", pulse.d);
          }
        }
      }
      this.demoSignalFrame = requestAnimationFrame(tick);
    };
    this.demoSignalFrame = requestAnimationFrame(tick);
  }

  stopDemoSignalLoop() {
    if (this.demoSignalFrame) cancelAnimationFrame(this.demoSignalFrame);
    this.demoSignalFrame = 0;
    this.demoSignalStartedAt = 0;
    this.demoSignalPaths = null;
    this.lastTopologyPaintKey = null;
  }

  scheduleTopologyPaint() {
    if (this.mapScrollFrame || !this.isConnected) return;
    this.mapScrollFrame = requestAnimationFrame(() => {
      this.mapScrollFrame = 0;
      this.drawTopology();
    });
  }

  drawTopology(force = false) {
    const canvas = this.querySelector(".map-lightfield");
    if (this.previousLightfield && this.previousLightfield !== canvas) {
      releaseTopology(this.previousLightfield);
      this.lastTopologyPaintKey = null;
    }
    this.previousLightfield = canvas;
    if (!canvas || !this.visibleGraph) return;
    const scrollY = 0;
    const height = Math.max(1, this.visibleGraph.height);
    const demoPulse = this.demoPulse();
    // Chat deltas may arrive many times per second. Geometry, selection,
    // viewport and confirmed pulse—not transcript traffic—drive the shader.
    const key = JSON.stringify([
      this.visibleGraph.nodes.map((node) =>
        [node.id, node.x, node.y, node.w, node.h, node.status]),
      this.visibleGraph.edges.map((edge) => [edge.d, edge.stale]),
      this.fleet.selected?.hostId, this.fleet.selected?.uri, this.previewNodeId,
      this.fleet.lastSignal?.at, this.fleet.slots.chat?.uri,
      demoPulse?.edge?.d, demoPulse?.progress,
      height, this.mapZoom || 1, this.motionPreference?.matches,
    ]);
    if (!force && this.lastTopologyPaintKey === key) return;
    this.lastTopologyPaintKey = key;
    paintTopology(canvas, this.visibleGraph, this.fleet.selected,
      this.fleet.lastSignal, this.fleet.slots.chat?.uri, scrollY, height,
      this.previewNodeId, "", this.mapZoom || 1);
  }

  disconnectedCallback() {
    if (this.mapScrollFrame) cancelAnimationFrame(this.mapScrollFrame);
    this.stopDemoSignalLoop();
    this.devReload?.close();
    this.devReload = null;
    releaseTopology(this.previousLightfield);
    document.removeEventListener("keydown", this.handleShortcuts);
    document.removeEventListener("visibilitychange", this.handleVisibility);
    window.removeEventListener("resize", this.handleTopologyResize);
    this.motionPreference?.removeEventListener("change", this.handleMotionChange);
    super.disconnectedCallback();
    void this.fleet.shutdown();
  }

  openShortcuts() {
    const dialog = this.querySelector(".shortcuts-dialog");
    if (dialog && !dialog.open) dialog.showModal();
  }

  openCatalog() {
    this.catalogOpen = true;
    this.requestUpdate();
    void this.updateComplete.then(() => this.querySelector(".session-row")?.focus());
  }

  onSessionTabKey(event, mode) {
    const modes = ["chat", "files", "details"];
    const index = modes.indexOf(mode);
    const next = event.key === "ArrowRight" || event.key === "ArrowDown"
      ? (index + 1) % modes.length
      : event.key === "ArrowLeft" || event.key === "ArrowUp"
        ? (index + modes.length - 1) % modes.length
        : event.key === "Home" ? 0
          : event.key === "End" ? modes.length - 1 : -1;
    if (next < 0) return;
    event.preventDefault();
    this.sessionTab = modes[next];
    this.requestUpdate();
    void this.updateComplete.then(() => this.querySelector(`#${modes[next]}-tab`)?.focus());
  }

  openConnectForm() {
    if (this.drawerMode) this.closeDrawer();
    if (this.hostPanelOpen && window.innerWidth < 762) this.hostPanelOpen = false;
    this.connectOpen = true;
    this.pendingFocus = "connect";
    this.requestUpdate();
  }

  closeConnectForm() {
    this.connectOpen = false;
    this.requestUpdate();
    void this.updateComplete.then(() => this.querySelector(".bar-add-host")?.focus());
  }

  toggleHostPanel() {
    if (this.hostPanelOpen) this.closeHostPanel();
    else {
      this.hostPanelOpen = true;
      this.requestUpdate();
      void this.updateComplete.then(() => this.querySelector(".rail-close")?.focus());
    }
  }

  closeHostPanel() {
    this.hostPanelOpen = false;
    this.requestUpdate();
    void this.updateComplete.then(() => this.querySelector(".host-panel-toggle")?.focus());
  }

  beginPanelResize(event, side) {
    if (event.button !== 0 && event.pointerType !== "touch") return;
    event.preventDefault();
    event.stopPropagation();
    this.panelResize = { id: event.pointerId, side };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  }

  movePanelResize(event) {
    if (!this.panelResize || this.panelResize.id !== event.pointerId) return;
    const maxW = Math.max(224, window.innerWidth - 64);
    if (this.panelResize.side === "left") {
      this.leftPanelWidth = Math.max(180, Math.min(maxW, Math.round(event.clientX)));
    } else {
      this.rightDrawerWidth = Math.max(180, Math.min(maxW, Math.round(window.innerWidth - event.clientX)));
    }
    this.requestUpdate();
  }

  endPanelResize(event) {
    if (!this.panelResize || this.panelResize.id !== event.pointerId) return;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    this.panelResize = null;
  }

  frameMapToGraph() {
    const graph = this.visibleGraph;
    const viewport = this.querySelector(".map-viewport");
    const stage = this.querySelector(".map-stage");
    if (!graph?.nodes.length || !viewport) return;
    const key = graph.nodes.map((node) =>
      [node.id, node.x, node.y, node.w, node.h]).flat().join("|");
    if (key === this.mapFrameKey) return;
    const minX = Math.min(...graph.nodes.map((node) => node.x));
    const maxX = Math.max(...graph.nodes.map((node) => node.x + node.w));
    const minY = Math.min(...graph.nodes.map((node) => node.y));
    const maxY = Math.max(...graph.nodes.map((node) => node.y + node.h));
    const focusNode = viewport.clientWidth < 760
      ? graph.nodes.find((node) => node.kind === "host") || graph.nodes[0]
      : null;
    const focusX = focusNode
      ? focusNode.x + focusNode.w / 2
      : (minX + maxX) / 2;
    const focusY = (minY + maxY) / 2;
    this.mapZoom = 1;
    const restingTop = stage ? stage.offsetTop : 35;
    const targetVx = viewport.clientWidth / 2;
    const targetVy = viewport.clientHeight / 2;
    const stageLeft = targetVx - focusX;
    const stageTop = targetVy - focusY;
    const targetPanX = Math.round(stageLeft - (viewport.clientWidth - graph.width) / 2);
    const targetPanY = Math.round(stageTop - restingTop);
    const clamped = this.clampMapPan(targetPanX, targetPanY, viewport);
    this.mapPanX = clamped.x;
    this.mapPanY = clamped.y;
    this.mapFrameKey = key;
    this.updateMapTransform();
  }

  fitMapToCanvas() {
    const graph = this.visibleGraph;
    const viewport = this.querySelector(".map-viewport");
    const stage = this.querySelector(".map-stage");
    if (!graph?.nodes.length || !viewport) return;
    const minX = Math.min(...graph.nodes.map((node) => node.x));
    const maxX = Math.max(...graph.nodes.map((node) => node.x + node.w));
    const minY = Math.min(...graph.nodes.map((node) => node.y));
    const maxY = Math.max(...graph.nodes.map((node) => node.y + node.h));
    const boxW = Math.max(1, maxX - minX + 64);
    const boxH = Math.max(1, maxY - minY + 64);
    const availW = Math.max(120, viewport.clientWidth - 48);
    const availH = Math.max(120, viewport.clientHeight - 110);
    const nextZoom = Math.max(0.4, Math.min(2.0, Math.min(availW / boxW, availH / boxH)));
    this.mapZoom = Math.round(nextZoom * 100) / 100;
    const nodeCenterX = (minX + maxX) / 2;
    const nodeCenterY = (minY + maxY) / 2;
    const restingTop = stage ? stage.offsetTop : 35;
    const targetVx = viewport.clientWidth / 2;
    const targetVy = (viewport.clientHeight + 24) / 2;
    const stageLeft = targetVx - nodeCenterX * this.mapZoom;
    const stageTop = targetVy - nodeCenterY * this.mapZoom;
    const targetPanX = Math.round(stageLeft - (viewport.clientWidth - graph.width * this.mapZoom) / 2);
    const targetPanY = Math.round(stageTop - restingTop);
    const clamped = this.clampMapPan(targetPanX, targetPanY, viewport);
    this.mapPanX = clamped.x;
    this.mapPanY = clamped.y;
    this.updateMapTransform();
    this.requestUpdate();
  }

  zoomMapBy(delta, pivotX, pivotY) {
    const viewport = this.querySelector(".map-viewport");
    const stage = this.querySelector(".map-stage");
    const prevZoom = this.mapZoom || 1;
    const nextZoom = Math.max(0.4, Math.min(2.4, Math.round((prevZoom + delta) * 100) / 100));
    if (nextZoom === prevZoom) return;
    if (viewport && this.visibleGraph) {
      const rect = viewport.getBoundingClientRect();
      const vx = pivotX != null ? pivotX - rect.left : viewport.clientWidth / 2;
      const vy = pivotY != null ? pivotY - rect.top : viewport.clientHeight / 2;
      const restingTop = stage ? stage.offsetTop : 35;
      const stageLeft = (viewport.clientWidth - this.visibleGraph.width * prevZoom) / 2 + this.mapPanX;
      const stageTop = restingTop + this.mapPanY;
      const graphX = (vx - stageLeft) / prevZoom;
      const graphY = (vy - stageTop) / prevZoom;
      const newStageLeft = vx - graphX * nextZoom;
      const newStageTop = vy - graphY * nextZoom;
      const targetPanX = Math.round(newStageLeft - (viewport.clientWidth - this.visibleGraph.width * nextZoom) / 2);
      const targetPanY = Math.round(newStageTop - restingTop);
      this.mapZoom = nextZoom;
      const clamped = this.clampMapPan(targetPanX, targetPanY, viewport);
      this.mapPanX = clamped.x;
      this.mapPanY = clamped.y;
    } else {
      this.mapZoom = nextZoom;
    }
    this.updateMapTransform();
    this.requestUpdate();
  }

  onMapWheel(event) {
    if (event.ctrlKey || event.metaKey || event.deltaY !== 0) {
      event.preventDefault();
      const step = event.deltaY < 0 ? 0.12 : -0.12;
      this.zoomMapBy(step, event.clientX, event.clientY);
    }
  }

  clampMapPan(x, y, viewport = this.querySelector(".map-viewport")) {
    const graph = this.visibleGraph;
    if (!graph || !viewport) return { x, y };
    const zoom = this.mapZoom || 1;
    const padding = graph.padding || { x: 72, y: 72 };
    const maxX = Math.max(padding.x, (graph.width * zoom - viewport.clientWidth) / 2 + padding.x);
    const maxY = Math.max(padding.y, (graph.height * zoom - viewport.clientHeight) / 2 + padding.y);
    return {
      x: Math.max(-maxX, Math.min(maxX, x)),
      y: Math.max(-maxY, Math.min(maxY, y)),
    };
  }

  updateMapTransform() {
    const stage = this.querySelector(".map-stage");
    if (!stage) return;
    const graph = this.visibleGraph;
    const zoom = this.mapZoom || 1;
    if (graph) {
      stage.style.width = `${Math.round(graph.width * zoom)}px`;
      stage.style.height = `${Math.round(graph.height * zoom)}px`;
    }
    stage.style.transform = `translate3d(${Math.round(this.mapPanX)}px, ${Math.round(this.mapPanY)}px, 0)`;
    stage.dataset.zoom = String(zoom);
    stage.classList.toggle("is-panning", Boolean(this.mapPointer));
    this.scheduleTopologyPaint();
  }

  beginMapPan(event) {
    if (event.button !== 0 && event.pointerType !== "touch") return;
    const nodeTarget = event.target.closest?.(".node");
    if (nodeTarget && !this.spaceHeld) return;
    const viewport = event.currentTarget;
    const origin = this.clampMapPan(this.mapPanX, this.mapPanY, viewport);
    this.mapPanX = origin.x;
    this.mapPanY = origin.y;
    this.mapPointer = {
      id: event.pointerId, x: event.clientX, y: event.clientY,
      originX: origin.x, originY: origin.y,
    };
    this.mapDidPan = false;
    viewport.setPointerCapture?.(event.pointerId);
    this.updateMapTransform();
    event.preventDefault();
  }

  moveMapPan(event) {
    if (!this.mapPointer || event.pointerId !== this.mapPointer.id) return;
    const dx = event.clientX - this.mapPointer.x;
    const dy = event.clientY - this.mapPointer.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) this.mapDidPan = true;
    const next = this.clampMapPan(this.mapPointer.originX + dx, this.mapPointer.originY + dy,
      event.currentTarget);
    this.mapPanX = next.x;
    this.mapPanY = next.y;
    this.updateMapTransform();
    if (this.mapDidPan) event.preventDefault();
  }

  endMapPan(event) {
    if (!this.mapPointer || event.pointerId !== this.mapPointer.id) return;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    this.mapPointer = null;
    this.updateMapTransform();
    if (this.mapDidPan) this.suppressMapClickUntil = performance.now() + 80;
  }

  onMapKeyDown(event) {
    if (event.key !== " ") return;
    if (event.target.closest?.(".node")) return;
    this.spaceHeld = true;
    event.preventDefault();
  }

  onMapKeyUp(event) {
    if (event.key !== " ") return;
    this.spaceHeld = false;
    event.preventDefault();
  }

  openDrawer(mode) {
    this.drawerMode = mode;
    this.requestUpdate();
  }

  closeDrawer() {
    this.drawerMode = "";
    this.querySelector(".side-drawer")?.close();
    this.requestUpdate();
  }

  activateNode(node) {
    this.previewNode(node);
  }

  previewNode(node) {
    if (!node) return;
    this.previewNodeId = node.id;
    if (node.kind === "session") {
      this.prepareSession(node.hostId, node.uri);
      return;
    }
    this.activeHost = node.hostId;
    this.requestUpdate();
  }

  closePreview() {
    this.previewNodeId = "";
    if (!this.drawerMode && this.fleet.selected) {
      void this.run(() => this.fleet.select(null));
    }
    this.requestUpdate();
  }

  openNode(node) {
    if (!node) return;
    this.previewNodeId = node.id;
    if (node.kind === "session") this.openSession(node.hostId, node.uri);
    else this.selectHost(node.hostId);
  }

  onGraphKey(event, node, graph) {
    if (event.key === "Enter") {
      event.preventDefault();
      this.openNode(node);
      return;
    }
    if (event.key === "Escape" && (this.previewNodeId || this.fleet.selected)) {
      event.preventDefault();
      this.closePreview();
      this.closeDrawer();
      return;
    }
    const direction = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" }[event.key];
    if (!direction && event.key !== "Home" && event.key !== "End") return;
    const origin = { x: node.x + node.w / 2, y: node.y + node.h / 2 };
    const candidates = graph.nodes.map((other, index) => {
      const dx = other.x + other.w / 2 - origin.x;
      const dy = other.y + other.h / 2 - origin.y;
      if (other.id === node.id ||
          (direction === "left" && dx >= -1) || (direction === "right" && dx <= 1) ||
          (direction === "up" && dy >= -1) || (direction === "down" && dy <= 1)) return null;
      const horizontal = direction === "left" || direction === "right";
      const primary = Math.abs(horizontal ? dx : dy);
      const transverse = Math.abs(horizontal ? dy : dx);
      return { index, score: primary + transverse * (horizontal ? 2.5 : 3) };
    }).filter(Boolean);
    const next = event.key === "Home"
      ? graph.nodes.findIndex((entry) => entry.kind === "host")
      : event.key === "End"
        ? Math.max(0, graph.nodes.findLastIndex((entry) => entry.kind === "session"))
        : candidates.sort((a, b) => a.score - b.score)[0]?.index;
    if (next === undefined || next < 0) return;
    event.preventDefault();
    this.focusedNodeId = graph.nodes[next].id;
    this.requestUpdate();
    event.currentTarget.parentElement.querySelector(`[data-node-index="${next}"]`)?.focus();
  }

  async run(action) {
    if (this.busy) return;
    this.busy = true;
    this.fleet.error = "";
    this.requestUpdate();
    try { await action(); }
    catch (error) { this.fleet.error = safeError(error?.message || error); }
    finally { this.busy = false; this.requestUpdate(); }
  }

  selectHost(id) {
    if (this.busy) return;
    if (this.activeHost !== id) {
      this.modelId = "";
      this.sessionConfig = null;
      ++this.configRevision;
      this.historyOffset = 0;
      this.explicitChatUri = "";
      this.filePreview = null;
    }
    this.activeHost = id;
    this.openDrawer("host");
    this.requestUpdate();
  }

  removeActiveHost(hostId) {
    void this.run(async () => {
      await this.fleet.removeHost(hostId);
      const next = this.fleet.hosts()[0];
      this.activeHost = next?.id || "";
      this.modelId = "";
      this.filePreview = null;
      this.historyOffset = 0;
      this.explicitChatUri = "";
      this.closeDrawer();
      this.sessionConfig = null;
      this.sessionConfigKey = "";
      ++this.configRevision;
    });
  }

  prepareSession(hostId, uri) {
    if (this.busy) return;
    if (this.fleet.selected?.hostId !== hostId || this.fleet.selected?.uri !== uri) {
      this.modelId = "";
      this.historyOffset = 0;
      this.explicitChatUri = "";
      this.filePreview = null;
      this.sessionTab = "chat";
    }
    this.activeHost = hostId;
    void this.run(() => this.fleet.select({ hostId, uri }));
  }

  openSession(hostId, uri) {
    this.previewNodeId = hostId + "\0" + uri;
    this.prepareSession(hostId, uri);
    this.openDrawer("session");
  }

  setNodeViewHover(nodeId, view) {
    const state = this.nodeViewState.get(nodeId) || {};
    if (state.hover === view) return;
    this.nodeViewState.set(nodeId, { ...state, hover: view });
    this.requestUpdate();
  }

  clearNodeViewHover(nodeId) {
    const state = this.nodeViewState.get(nodeId);
    if (!state?.hover) return;
    this.nodeViewState.set(nodeId, { ...state, hover: "" });
    this.requestUpdate();
  }

  toggleNodeView(nodeId, view) {
    const state = this.nodeViewState.get(nodeId) || {};
    this.nodeViewState.set(nodeId, {
      ...state,
      pinned: state.pinned === view ? "" : view,
    });
    this.requestUpdate();
  }

  onNodeToolKey(event, node, view) {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    event.stopPropagation();
    this.toggleNodeView(node.id, view);
  }

  connect(event) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const form = event.currentTarget;
    void this.run(async () => {
      await this.fleet.addHost({
        label: data.get("label") || "",
        endpoint: data.get("endpoint") || "",
        token: data.get("token") || "",
      });
      this.activeHost = String(data.get("label")).trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
      form.reset();
      this.connectOpen = false;
      this.closeDrawer();
    });
  }

  toggleCreate() {
    if (this.drawerMode === "create") { this.closeDrawer(); return; }
    this.openDrawer("create");
    this.sessionConfig = null;
    this.sessionConfigKey = "";
    ++this.configRevision;
    this.requestUpdate();
    void this.updateComplete.then(() => this.resolveCreateConfig(this.querySelector(".create-form"), true));
  }

  readSessionConfig(data) {
    const schema = this.sessionConfig?.schema;
    const values = { ...(this.sessionConfig?.values || {}) };
    for (const [key, property] of Object.entries(schema?.properties || {})) {
      if (property.readOnly) continue;
      const raw = data.get("config:" + key);
      if (raw === null) continue;
      if (property.type === "object" || property.type === "array" &&
          property.items?.type !== "string") {
        if (raw.trim() === "") continue;
        let parsed;
        try { parsed = JSON.parse(raw); }
        catch { throw Error("Invalid JSON for host option " + (property.title || key)); }
        if (property.type === "object"
          ? parsed === null || typeof parsed !== "object" || Array.isArray(parsed)
          : !Array.isArray(parsed)) {
          throw Error("Invalid type for host option " + (property.title || key));
        }
        values[key] = parsed;
      } else if (property.type === "array" && property.items?.type === "string") {
        values[key] = String(raw).split(/\r?\n/).map((item) => item.trim()).filter(Boolean);
      } else if (raw === "") continue;
      else if (property.enum?.length && !property.enumDynamic) {
        const index = Number(raw);
        if (!Number.isInteger(index) || index < 0 || index >= property.enum.length) {
          throw Error("Invalid host option " + (property.title || key));
        }
        values[key] = property.enum[index];
      } else if (property.type === "boolean") {
        values[key] = raw === "true";
      } else if (property.type === "number") {
        const value = Number(raw);
        if (!Number.isFinite(value)) throw Error("Invalid host option " + (property.title || key));
        values[key] = value;
      } else if (property.type === "string") {
        values[key] = String(raw);
      }
    }
    return values;
  }

  async resolveCreateConfig(form, reset = false) {
    if (!form || this.busy || this.drawerMode !== "create") return;
    const data = new FormData(form);
    const hostId = this.activeHost;
    const provider = String(data.get("provider") || "");
    const directory = String(data.get("directory") || "");
    if (!provider) return;
    const key = JSON.stringify([hostId, provider, directory]);
    const config = reset ? {} : this.readSessionConfig(data);
    const revision = ++this.configRevision;
    await this.run(async () => {
      const result = await this.fleet.resolveSessionConfig({ hostId, provider, directory, config });
      if (this.configRevision === revision && this.drawerMode === "create" && this.activeHost === hostId) {
        this.sessionConfig = result;
        this.sessionConfigKey = key;
      }
    });
  }

  createFormChanged(event) {
    const name = event.target.name || "";
    if (name === "provider") {
      this.sessionConfig = null;
      void this.resolveCreateConfig(event.currentTarget, true);
    } else if (name === "directory") {
      // The submit action resolves workspace-dependent schema itself. Starting
      // an RPC during input blur would steal the user's first submit click.
      this.sessionConfig = null;
      this.sessionConfigKey = "";
      ++this.configRevision;
      this.requestUpdate();
    } else if (name.startsWith("config:") && event.target.tagName === "SELECT") {
      void this.resolveCreateConfig(event.currentTarget);
    }
  }

  renderConfigField(key, property) {
    const value = this.sessionConfig.values?.[key] ?? property.default;
    const required = (this.sessionConfig.schema?.required || []).includes(key);
    const title = property.title || key;
    if (property.readOnly || !["string", "number", "boolean", "array", "object"].includes(property.type)) {
      return html`<div class="host-option host-option-info"><strong>${title}</strong>
        <span>${property.readOnly ? "Managed by host" : "Advanced host setting"}${required ? " · Required" : ""}</span>
        ${property.description ? html`<small>${property.description}</small>` : nothing}
      </div>`;
    }
    let input;
    if (property.type === "object" || property.type === "array" &&
        property.items?.type !== "string") {
      input = html`<textarea name="config:${key}" rows="4" spellcheck="false"
        placeholder="JSON value" ?required=${required}
        .value=${JSON.stringify(value ?? (property.type === "array" ? [] : {}), null, 2)}></textarea>`;
    } else if (property.type === "array") {
      input = html`<textarea name="config:${key}" rows="3" spellcheck="false"
        placeholder="One entry per line" ?required=${required}
        .value=${Array.isArray(value) ? value.join("\n") : ""}></textarea>`;
    } else if (Array.isArray(property.enum) && !property.enumDynamic) {
      const index = property.enum.findIndex((candidate) => Object.is(candidate, value));
      input = html`<select name="config:${key}" .value=${index < 0 ? "" : String(index)} ?required=${required}>
        <option value="">${required ? "Choose an option" : "Host default"}</option>
        ${property.enum.map((choice, position) => html`<option value="${position}">${property.enumLabels?.[position] || String(choice)}</option>`)}
      </select>`;
    } else if (property.type === "boolean") {
      input = html`<select name="config:${key}" .value=${value === undefined ? "" : String(value)} ?required=${required}>
        <option value="">${required ? "Choose an option" : "Host default"}</option>
        <option value="true">Yes</option><option value="false">No</option>
      </select>`;
    } else {
      input = html`<input name="config:${key}" .value=${value == null ? "" : String(value)}
        type="${property.type === "number" ? "number" : "text"}" step=${property.type === "number" ? "any" : nothing}
        ?required=${required} spellcheck="false" autocomplete="off"
        list=${property.enumDynamic && property.enum?.length ? "choices-" + key : nothing}>
        ${property.enumDynamic && property.enum?.length ? html`
          <datalist id="choices-${key}">${property.enum.map((choice, position) =>
            html`<option value="${String(choice)}" label="${property.enumLabels?.[position] || String(choice)}"></option>`)}</datalist>` : nothing}`;
    }
    return html`<label class="host-option">${title}${input}
      ${property.description ? html`<small>${property.description}</small>` : nothing}
    </label>`;
  }

  create(event) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    void this.run(async () => {
      const hostId = this.activeHost;
      const provider = String(data.get("provider") || "");
      const directory = String(data.get("directory") || "");
      const trust = data.get("trustWorkspace") === "on";
      const key = JSON.stringify([hostId, provider, directory]);
      let config;
      if (this.sessionConfig && this.sessionConfigKey === key) {
        const values = this.readSessionConfig(data);
        const result = await this.fleet.resolveSessionConfig({ hostId, provider, directory, config: values });
        this.sessionConfig = result;
        this.sessionConfigKey = key;
        config = { ...result.values, ...values };
      } else {
        const result = await this.fleet.resolveSessionConfig({ hostId, provider, directory });
        this.sessionConfig = result;
        this.sessionConfigKey = key;
        config = { ...(result.values || {}) };
      }
      const required = this.sessionConfig.schema?.required || [];
      if (required.some((field) => config[field] == null || config[field] === "" ||
          (Array.isArray(config[field]) && this.sessionConfig.schema.properties?.[field]?.minItems > config[field].length))) {
        throw Error("Complete the host-required session options");
      }
      await this.fleet.createSession({
        hostId, provider, directory, trust, ...(config ? { config } : {}),
      });
      this.previewNodeId = this.fleet.selected
        ? this.fleet.selected.hostId + "\0" + this.fleet.selected.uri : "";
      this.modelId = "";
      this.sessionConfig = null;
      this.sessionTab = "chat";
      this.openDrawer("session");
    });
  }

  submitPrompt(event) {
    event.preventDefault();
    const input = event.currentTarget.elements.namedItem("prompt");
    const message = input?.value || "";
    void this.run(() => {
      this.fleet.queueTurn(message, this.modelId || undefined);
      input.value = "";
    });
  }

  answer(event, entry) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    void this.run(() => {
      const answers = {};
      for (const question of entry.request.questions || []) {
        answers[question.id] = readAnswer(question, data);
      }
      this.fleet.answerInput(entry, "accept", answers);
    });
  }

  confirm(entry, approved, selectedOptionId) {
    void this.run(() => this.fleet.confirmTool(entry, approved, selectedOptionId));
  }

  refreshSelected() {
    this.filePreview = null;
    void this.run(() => this.fleet.refreshSelection());
  }

  async inspectFile(fileId) {
    const { hostId, uri } = this.fleet.selected || {};
    const changesetUri = this.fleet.slots.changes?.uri;
    const previous = this.filePreview;
    if (previous?.hostId === hostId && previous?.sessionUri === uri &&
        previous?.changesState === this.fleet.changes &&
        previous?.changesetUri === changesetUri && previous.fileId === fileId) {
      this.filePreview = null;
      this.requestUpdate();
      return;
    }
    const preview = {
      hostId, sessionUri: uri, changesetUri, fileId, changesState: this.fleet.changes,
      before: null, after: null, loading: true, error: "",
    };
    this.filePreview = preview;
    this.requestUpdate();
    try {
      const result = await this.fleet.readEdit(fileId);
      if (this.filePreview !== preview || preview.changesState !== this.fleet.changes) return;
      preview.before = result.before;
      preview.after = result.after;
    } catch (error) {
      if (this.filePreview !== preview) return;
      preview.error = error?.message || String(error);
    } finally {
      if (this.filePreview === preview) {
        preview.loading = false;
        this.requestUpdate();
      }
    }
  }

  changeTurnForChangeset(field, value) {
    const params = { ...this.fleet.changesetParams, [field]: value };
    const source = this.fleet.changeset?.sourceTemplate;
    const choice = this.fleet.availableChangesets(params).find((entry) =>
      entry.sourceTemplate === source);
    if (!choice) return;
    this.filePreview = null;
    void this.run(() => this.fleet.selectChangeset(choice.uriTemplate, undefined, params));
  }

  async earlierTurns(chat) {
    const end = Math.max(0, chat.turns.length - this.historyOffset);
    const start = Math.max(0, end - 40);
    if (start > 0) {
      this.historyOffset = chat.turns.length - start;
      this.requestUpdate();
    } else if (chat.turnsNextCursor) {
      const previouslyLoaded = chat.turns.length;
      await this.run(async () => {
        await this.fleet.loadOlderTurns();
        const current = this.fleet.chat?.turns.length || 0;
        if (current > previouslyLoaded) this.historyOffset = previouslyLoaded;
      });
    }
  }

  getView() {
    const hosts = this.fleet.eventStreamHealthy
      ? this.fleet.hosts()
      : this.fleet.hosts().map((host) => ({ ...host, state: { status: "stale" } }));
    const sessions = this.fleet.sessions();
    const activeHost = hosts.find((host) => host.id === this.activeHost) || hosts[0] || null;
    const selectedSummary = sessions.find((s) =>
      this.fleet.selected?.hostId === s.hostId &&
      this.fleet.selected?.uri === s.summary.resource)?.summary;
    const working = sessions.filter((s) =>
      sessionState(s.summary, hosts.find((h) => h.id === s.hostId)?.state.status === "connected") === "working").length;
    const attention = sessions.filter((s) => statusOf(s.summary.status) === "attention" &&
      hosts.find((h) => h.id === s.hostId)?.state.status === "connected").length;
    return { hosts, sessions, activeHost, selectedSummary, working, attention };
  }

  render() {
    const { hosts, sessions, activeHost, selectedSummary } = this.getView();
    const online = hosts.filter((host) => host.state.status === "connected").length;
    const graph = buildGraph(hosts, sessions, this.filterText);
    this.visibleGraph = graph;
    const demoEdges = this.demoPulses();
    const previewNode = graph.nodes.find((node) => node.id === this.previewNodeId);
    const focusedNode = graph.nodes.find((node) => node.id === this.focusedNodeId) ||
      graph.nodes.find((node) => node.kind === "session" &&
        node.hostId === this.fleet.selected?.hostId && node.uri === this.fleet.selected?.uri) ||
      graph.nodes.find((node) => node.kind === "host") || graph.nodes[0];
    const selected = this.fleet.selected;
    const previewSummary = previewNode?.kind === "session"
      ? sessions.find((row) => row.hostId === previewNode.hostId &&
        row.summary.resource === previewNode.uri)?.summary : null;
    const previewHost = previewNode ? hosts.find((host) => host.id === previewNode.hostId) : null;
    const previewAgent = previewNode?.kind === "provider"
      ? previewHost?.agents.find((agent) => agent.provider === previewNode.title ||
        agent.displayName === previewNode.title) : null;
    const live = this.fleet.currentHost()?.state.status === "connected";
    const detail = this.fleet.session;
    const chat = this.fleet.chat;
    const selectableChats = (detail?.chats || []).filter((entry) =>
      entry.interactivity !== "hidden");
    const promptPolicy = this.fleet.chatPromptPolicy;
    const chatAllowsInput = promptPolicy.allowsInput;
    const turns = chat?.turns || [];
    const historyEnd = Math.max(0, turns.length - this.historyOffset);
    const historyStart = Math.max(0, historyEnd - 40);
    const models = selected
      ? this.fleet.modelsFor(selected.hostId, detail?.provider || selectedSummary?.provider) : [];
    const changesets = this.fleet.availableChangesets();
    const changesetCatalog = this.fleet.changesetCatalog();
    const changes = this.fleet.changes;
    const changeset = this.fleet.changeset;
    const turnIds = turns.filter((turn) => typeof turn.id === "string");
    const changesetTemplate = changeset?.sourceTemplate || changesets[0]?.sourceTemplate || "";
    const changesLoadError = this.fleet.slots.changes?.error;
    const preview = this.filePreview?.hostId === selected?.hostId &&
      this.filePreview?.sessionUri === selected?.uri &&
      this.filePreview?.changesState === changes &&
      this.filePreview?.changesetUri === this.fleet.slots.changes?.uri &&
      changes?.files?.some((file) => file.id === this.filePreview.fileId)
        ? this.filePreview : null;
    const canReviewChanges = changeset?.capabilities &&
      Object.prototype.hasOwnProperty.call(changeset.capabilities, "review");
    const sessionLoadError = this.fleet.slots.session?.error;
    const chatLoadError = this.fleet.slots.chat?.error;
    const filteredSessions = sessions.filter((s) => {
      const value = [s.hostLabel, s.summary.title, s.summary.provider,
        ...(s.summary.workingDirectories || [])].join(" ").toLowerCase();
      return this.filterText.toLowerCase().split(/\s+/).filter(Boolean)
        .every((term) => value.includes(term));
    });
    const hostById = new Map(hosts.map((host) => [host.id, host]));
    const sessionByNodeId = new Map(sessions.map((row) =>
      [row.hostId + "\0" + row.summary.resource, row.summary]));
    const nodeViewDetails = (node, view) => {
      const host = hostById.get(node.hostId);
      const summary = sessionByNodeId.get(node.id);
      if (node.kind === "session" && view === "state") return {
        label: "STATE", value: summary?.activity || nodeStateLabel(node.status) || String(node.status || "IDLE").toUpperCase(),
      };
      if (node.kind === "host" && view === "state") return {
        label: "ENDPOINT", value: host?.endpoint ? tail(host.endpoint) : (nodeStateLabel(node.status) || String(node.status || "UNKNOWN").toUpperCase()),
      };
      if (view === "state") return {
        label: "STATE", value: nodeStateLabel(node.status) || String(node.status || "UNKNOWN").toUpperCase(),
      };
      if (node.kind === "session" && view === "activity") return {
        label: "ACTIVITY", value: summary?.activity || "No active turn",
      };
      if (node.kind === "session" && view === "workspace") return {
        label: "WORKSPACE", value: tail(summary?.workingDirectories?.[0]),
      };
      if (node.kind === "session" && view === "provider") return {
        label: "AGENT", value: summary?.provider || "Unknown agent",
      };
      if (node.kind === "provider" && view === "sessions") return {
        label: "SESSIONS", value: `${node.sessionCount || 0} attached`,
      };
      if (node.kind === "provider" && view === "identity") return {
        label: "PROVIDER", value: node.provider || node.title,
      };
      if (node.kind === "host" && view === "agents") return {
        label: "AGENTS", value: `${node.agentCount || host?.agents?.length || 0} advertised`,
      };
      if (node.kind === "host" && view === "sessions") return {
        label: "SESSIONS", value: `${node.sessionCount || host?.sessionSummaries?.length || 0} visible`,
      };
      if (node.kind === "host" && view === "endpoint") return {
        label: "ENDPOINT", value: host?.endpoint ? tail(host.endpoint) : "AHP host",
      };
      return { label: "STATE", value: nodeStateLabel(node.status) || String(node.status || "UNKNOWN").toUpperCase() };
    };

    return html`
      <header class="bar">
        <div class="brand">
          <svg viewBox="0 0 26 26" aria-hidden="true"><path d="M2 3 H17 V10 H8 V23 H24 M17 3 H24 M8 16 H19" fill="none" stroke="currentColor" stroke-width="2"/></svg>
          <span>Relay</span><span class="brand-separator"></span><span class="brand-detail">Fleet</span>
        </div>
        <div class="bar-right">
          <span class="pulse-marker ${online ? "online" : ""}" aria-hidden="true"></span>
          <span role="status" aria-live="polite">${this.fleet.eventStreamHealthy
            ? `${online} / ${hosts.length} hosts online` : "AHP stream unavailable"}</span>
          ${this.fleet.error ? html`<button class="quiet small shell-error" type="button" @click=${() => this.openDrawer("status")}>${icon("warning")} Issue</button>` : nothing}
        </div>
      </header>
      <main id="main" class="workspace" data-left-width="${this.leftPanelWidth}px" data-right-width="${this.rightDrawerWidth}px">
        <button class="host-panel-toggle ${this.hostPanelOpen ? "is-open" : ""}" type="button"
          aria-controls="host-panel" aria-expanded=${this.hostPanelOpen}
          aria-label="Hosts (${hosts.length})"
          @click=${() => this.toggleHostPanel()}>
          <svg class="stripe-arrow" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M4 2.5L7.5 6 4 9.5" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>
          <span class="stripe-word" aria-hidden="true"><span>H</span><span>o</span><span>s</span><span>t</span><span>s</span></span>
          <span class="stripe-count" aria-hidden="true">${hosts.length}</span>
        </button>
        <aside id="host-panel" class="sidebar ${this.hostPanelOpen ? "is-open" : ""}" data-width="${this.leftPanelWidth}px" aria-label="Fleet hosts"
          aria-hidden=${this.hostPanelOpen ? nothing : "true"} .inert=${!this.hostPanelOpen}>
          <div class="rail-title"><span>HOSTS</span>
            <button class="quiet small rail-close" type="button" aria-label="Close hosts panel" @click=${() => this.closeHostPanel()}>${icon("close")}</button>
          </div>
          <div class="hosts-list">${hosts.map((host) => html`
            <button class="host-row ${activeHost?.id === host.id ? "selected" : ""}" @click=${() => this.selectHost(host.id)}
              aria-current=${activeHost?.id === host.id ? "true" : nothing} aria-label="${host.label}, ${host.state.status}, inspect host">
              <span class="status-dot ${host.state.status}" aria-hidden="true"></span>
              <span class="host-text"><strong>${host.label}</strong><small>${host.state.status}</small></span>
              <span class="subtle">${host.sessionSummaries?.length || 0}</span>
            </button>
          `)}</div>
          <div class="panel-resizer resizer-right" role="separator" aria-orientation="vertical" aria-label="Resize hosts panel"
            @pointerdown=${(e) => this.beginPanelResize(e, "left")}
            @pointermove=${(e) => this.movePanelResize(e)}
            @pointerup=${(e) => this.endPanelResize(e)}
            @pointercancel=${(e) => this.endPanelResize(e)}
            @click=${(e) => e.stopPropagation()}></div>
        </aside>
        ${this.hostPanelOpen ? html`<button class="panel-scrim" type="button" aria-label="Close hosts panel"
          @click=${() => this.closeHostPanel()}></button>` : nothing}

        <section class="center">
          <section class="topology-panel" aria-label="Topology">
            ${this.fleet.error && !this.drawerMode ? html`<div class="alert inline-alert" role="alert"><span>ERROR</span> ${safeError(this.fleet.error)}<button aria-label="Dismiss error" class="close" type="button" @click=${() => { this.fleet.error = ""; this.requestUpdate(); }}>${icon("close")}</button></div>` : nothing}
            <div class="topology-tools">
              <label class="sr-only" for="session-search">Search fleet</label>
              <input id="session-search" type="search" placeholder="Search hosts, agents, sessions" aria-keyshortcuts="/" .value=${this.filterText}
                @input=${(e) => { this.filterText = e.target.value; this.catalogLimit = 100; this.requestUpdate(); }}
                @keydown=${(e) => { if (e.key === "Escape" && this.filterText) { e.preventDefault(); this.filterText = ""; e.currentTarget.value = ""; this.requestUpdate(); } }}>
              ${this.filterText ? html`<button class="quiet small" @click=${() => { this.filterText = ""; this.requestUpdate(); this.querySelector("#session-search")?.focus(); }}>${icon("close")} Clear</button>` : nothing}
              <div class="map-zoom-controls" role="group" aria-label="Canvas zoom controls">
                <button class="quiet small zoom-btn" type="button" aria-label="Zoom out"
                  @click=${() => this.zoomMapBy(-0.15)}>−</button>
                <span class="zoom-readout" aria-live="polite">${Math.round((this.mapZoom || 1) * 100)}%</span>
                <button class="quiet small zoom-btn" type="button" aria-label="Zoom in"
                  @click=${() => this.zoomMapBy(0.15)}>+</button>
                <button class="quiet small fit-canvas-btn" type="button"
                  title="Fit to canvas" aria-label="Fit to canvas"
                  @click=${() => this.fitMapToCanvas()}>${icon("fit")}</button>
              </div>
            </div>
            <p class="sr-only" id="map-instructions">Use Tab to enter the topology, arrow keys to move between nodes, click to preview, Enter to open controls, and drag empty space to pan the canvas. The session catalog below offers the same sessions as a list.</p>
            <div class="map-viewport" role="region" aria-label="Pannable fleet topology" tabindex="0"
              @wheel=${(event) => this.onMapWheel(event)}
              @pointerdown=${(event) => this.beginMapPan(event)}
              @pointermove=${(event) => this.moveMapPan(event)}
              @pointerup=${(event) => this.endMapPan(event)}
              @pointercancel=${(event) => this.endMapPan(event)}
              @keydown=${(event) => this.onMapKeyDown(event)}
              @keyup=${(event) => this.onMapKeyUp(event)}>
              <div class="map-head ${this.connectOpen ? "is-connecting" : ""}">
                <div class="map-head-left" @pointerdown=${(event) => event.stopPropagation()}>
                  <button class="small bar-add-host ${this.connectOpen ? "is-active" : ""}" type="button"
                    aria-expanded=${this.connectOpen} aria-controls="connect-panel"
                    @click=${() => this.openConnectForm()}>${icon("plus")} Add host</button>
                  ${this.connectOpen ? html`
                    <form id="connect-panel" class="connect-form" @submit=${this.connect}
                      @keydown=${(event) => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); this.closeConnectForm(); } }}>
                      <label class="sr-only" for="connect-label">Host label</label>
                      <input id="connect-label" class="connect-field field-label" name="label" required placeholder="label" autocomplete="off" spellcheck="false">
                      <label class="sr-only" for="connect-endpoint">AHP WebSocket</label>
                      <input id="connect-endpoint" class="connect-field field-endpoint" name="endpoint" required type="url" placeholder="wss://host.example/ahp" autocomplete="off" spellcheck="false">
                      <label class="sr-only" for="connect-token">Access token</label>
                      <input id="connect-token" class="connect-field field-token" name="token" type="password" required placeholder="token"
                        aria-describedby="host-token-help" autocomplete="off">
                      <button class="primary small connect-submit" type="submit" ?disabled=${this.busy}>Connect ${icon("arrow-right")}</button>
                      <button class="quiet small connect-close" type="button" aria-label="Close connect form"
                        @click=${() => this.closeConnectForm()}>${icon("close")}</button>
                      <p class="sr-only" id="host-token-help">Use the access token issued by the host. Never share it.</p>
                    </form>
                  ` : nothing}
                </div>
              </div>
              ${hosts.length && graph.nodes.length ? html`
                <div class="map-stage"
                  style="width: ${Math.round(graph.width * (this.mapZoom || 1))}px; height: ${Math.round(graph.height * (this.mapZoom || 1))}px; transform: translate3d(${Math.round(this.mapPanX)}px, ${Math.round(this.mapPanY)}px, 0);"
                  data-zoom="${this.mapZoom || 1}">
                <canvas class="map-lightfield"
                  style="width: 100%; height: 100%;"
                  aria-hidden="true" @webglcontextrestored=${() => {
                    this.lastTopologyPaintKey = null;
                    this.scheduleTopologyPaint();
                  }}></canvas>
                <svg class="map" viewBox="0 0 ${graph.width} ${graph.height}" role="group" aria-label="AHP host to agent to session topology" aria-describedby="map-instructions">
                  <defs>
                    <pattern id="topology-grid" width="20" height="20" patternUnits="userSpaceOnUse"><path d="M 20 0 H 0 V 20" fill="none" stroke="#222" stroke-width=".5"></path></pattern>
                    <linearGradient id="signal-gradient" gradientUnits="userSpaceOnUse" x1="${Math.round(graph.width * 0.15)}" y1="${Math.round(graph.height * 0.15)}" x2="${Math.round(graph.width * 0.85)}" y2="${Math.round(graph.height * 0.85)}">
                      <stop offset="0%" stop-color="#c4ff00"></stop>
                      <stop offset="25%" stop-color="#96ff20"></stop>
                      <stop offset="50%" stop-color="#50ff60"></stop>
                      <stop offset="75%" stop-color="#20ffa0"></stop>
                      <stop offset="100%" stop-color="#00ffc4"></stop>
                    </linearGradient>
                    <linearGradient id="working-border-gradient" x1="0%" y1="0%" x2="100%" y2="100%">
                      <stop offset="0%" stop-color="#ffffff"></stop>
                      <stop offset="25%" stop-color="#a0a0a0"></stop>
                      <stop offset="50%" stop-color="#303030"></stop>
                      <stop offset="75%" stop-color="#a0a0a0"></stop>
                      <stop offset="100%" stop-color="#ffffff"></stop>
                      <animateTransform
                        attributeName="gradientTransform"
                        type="rotate"
                        from="0 0.5 0.5"
                        to="360 0.5 0.5"
                        dur="2.4s"
                        repeatCount="indefinite">
                      </animateTransform>
                    </linearGradient>
                    ${graph.edges.map((edge, edgeIndex) => {
                      const pts = edgeGradientCoords(edge.d);
                      return svg`<linearGradient id="edge-grad-${edgeIndex}" gradientUnits="userSpaceOnUse" x1="${pts.x1}" y1="${pts.y1}" x2="${pts.x2}" y2="${pts.y2}">
                        <stop offset="0%" stop-color="#c4ff00"></stop>
                        <stop offset="25%" stop-color="#96ff20"></stop>
                        <stop offset="50%" stop-color="#50ff60"></stop>
                        <stop offset="75%" stop-color="#20ffa0"></stop>
                        <stop offset="100%" stop-color="#00ffc4"></stop>
                      </linearGradient>`;
                    })}
                  </defs>
                  <rect class="map-grid" x="-10000" y="-10000" width="20000" height="20000" fill="url(#topology-grid)"></rect>
                  ${graph.edges.map((edge, edgeIndex) => svg`<path class="edge-casing" d="${edge.d}"></path><path class="edge ${edge.stale ? "stale" : ""} ${this.fleet.lastSignal && selected?.hostId === edge.hostId && selected?.uri === edge.sessionUri && this.fleet.lastSignal.uri === this.fleet.slots.chat?.uri ? "signal" : ""}" stroke="${edge.stale ? nothing : `url(#edge-grad-${edgeIndex})`}" d="${edge.d}"></path>`)}
                  ${demoEdges.map((pulse, i) => svg`<path class="demo-signal slot-${i}" data-slot="${i}" d="${pulse.d}" pathLength="1"></path>`)}
                  ${graph.nodes.map((node, index) => {
                    const state = this.nodeViewState.get(node.id) || {};
                    const activeView = state.hover || state.pinned || "";
                    const tools = nodeToolDefs(node);
                    const detail = activeView ? nodeViewDetails(node, activeView) : null;
                    const titleLines = nodeTitleLines(node);
                    const titleLayout = nodeTitleLayout(node, titleLines);
                    const signalLevel = nodeSignalLevel(node.status);
                    const railW = 16;
                    const railX = node.x + railW;
                    const contentX = railX + 6;
                    return svg`
                      <g class="node ${node.kind} ${node.status} ${node.advertised === false ? "unadvertised" : ""} ${activeHost?.id === node.hostId && node.kind === "host" ? "active-host" : ""} ${previewNode?.id === node.id ? "previewed" : ""} ${previewNode?.kind === "session" && selected?.hostId === node.hostId && selected?.uri === node.uri ? "chosen" : ""}"
                        role="button" tabindex="${node.id === focusedNode?.id ? "0" : "-1"}" data-node-index="${index}"
                        aria-label="${node.kind}: ${node.title}, ${node.subtitle}"
                        aria-current=${previewNode?.id === node.id ? "true" : nothing}
                        @focus=${() => { this.focusedNodeId = node.id; this.requestUpdate(); }}
                        @click=${(event) => {
                          if (this.suppressMapClickUntil > performance.now()) {
                            event.preventDefault();
                            event.stopPropagation();
                            return;
                          }
                          this.activateNode(node);
                        }}
                        @keydown=${(event) => this.onGraphKey(event, node, graph)}>
                        <rect class="node-body" x="${node.x}" y="${node.y}" width="${node.w}" height="${node.h}"></rect>
                        ${node.status === "working" ? svg`
                          <rect class="node-working-border" x="${node.x}" y="${node.y}" width="${node.w}" height="${node.h}" pathLength="100"></rect>
                        ` : nothing}
                        <rect class="node-rail-bg" x="${node.x + 1}" y="${node.y + 1}" width="${railW - 1}" height="${node.h - 2}"></rect>
                        <line class="node-tool-rail" x1="${railX}" y1="${node.y + 1}" x2="${railX}" y2="${node.y + node.h - 1}"></line>
                        ${tools.map((tool, toolIndex) => {
                          const railPad = 5.0;
                          const railStep = tools.length > 1 ? (node.h - railPad * 2) / (tools.length - 1) : 0;
                          const toolCy = node.y + railPad + toolIndex * railStep;
                          return svg`
                          <g class="node-tool ${state.pinned === tool.id ? "pinned" : ""}" role="button" tabindex="${node.id === focusedNode?.id ? "0" : "-1"}"
                            aria-label="${tool.label}" aria-pressed=${state.pinned === tool.id}
                            @pointerenter=${() => this.setNodeViewHover(node.id, tool.id)}
                            @pointerleave=${() => this.clearNodeViewHover(node.id)}
                            @click=${(event) => { event.stopPropagation(); this.toggleNodeView(node.id, tool.id); }}
                            @keydown=${(event) => this.onNodeToolKey(event, node, tool.id)}>
                            <rect class="node-tool-hit" x="${node.x + 1}" y="${node.y + toolIndex * (node.h / tools.length)}" width="${railW - 1}" height="${node.h / tools.length}"></rect>
                            ${nodeToolGlyph(tool.id, node.x + railW / 2 + 0.5, toolCy)}
                          </g>`;
                        })}
                        ${detail ? svg`
                          <text class="node-view-label" x="${contentX}" y="${node.y + 8.2}">${detail.label}</text>
                          <text class="node-view-value" x="${contentX}" y="${node.y + 14.4}">${short(detail.value, node.kind === "session" ? 15 : 16)}</text>
                        ` : svg`
                          <text class="node-name ${nodeTitleSizeClass(titleLayout.fontSize)} ${nodeStateLabel(node.status) ? "has-state" : ""}" x="${contentX}">
                            ${titleLines.map((line, lineIndex) => svg`
                              <tspan x="${contentX}" y="${titleLayout.firstBaseline + lineIndex * titleLayout.lineStep}">${line}</tspan>
                            `)}
                          </text>
                        `}
                        <g class="node-signal" aria-hidden="true">
                          ${[0, 1, 2, 3].map((slot) => svg`<rect class="${slot < signalLevel ? "on" : ""}" x="${contentX + slot * 12}" y="${node.y + node.h - 5.8}" width="9.5" height="2"></rect>`)}
                        </g>
                      </g>`;
                  })}
                </svg></div>
              ` : html`<div class="empty-map">${hosts.length ? "No matching nodes" : "No connected hosts"}</div>`}
            </div>
            ${graph.total > graph.shown ? html`<p class="map-note">
              <span>${graph.shown} / ${graph.total} sessions</span>
              <button class="map-overflow quiet small" type="button" @click=${() => this.openCatalog()}
                aria-controls="session-catalog" aria-expanded=${this.catalogOpen}>${icon("chevron-down")} Show ${graph.total - graph.shown} more</button>
            </p>` : nothing}
            ${previewNode ? html`
              <section class="node-preview" aria-live="polite" aria-label="Selected node preview">
                <div class="node-preview-head">
                  <div>
                    <span class="eyebrow">PREVIEW · ${previewNode.kind.toUpperCase()}</span>
                    <h2>${previewNode.title}</h2>
                  </div>
                  <button class="close preview-close" type="button" aria-label="Close preview" @click=${() => this.closePreview()}>
                    <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M2 2 L10 10 M10 2 L2 10" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>
                  </button>
                </div>
                <dl class="node-preview-details">
                  ${previewNode.kind === "session" ? html`
                    <div><dt>Host:</dt><dd>${previewHost?.label || previewNode.hostId}</dd></div>
                    <div><dt>Agent:</dt><dd>${previewSummary?.provider || "—"}</dd></div>
                    <div><dt>Workspace:</dt><dd title="${previewSummary?.workingDirectories?.[0] || ""}">${tail(previewSummary?.workingDirectories?.[0])}</dd></div>
                  ` : previewNode.kind === "provider" ? html`
                    <div><dt>Host:</dt><dd>${previewHost?.label || previewNode.hostId}</dd></div>
                    <div><dt>Sessions:</dt><dd>${sessions.filter((row) => row.hostId === previewNode.hostId && row.summary.provider === (previewAgent?.provider || previewNode.title)).length}</dd></div>
                  ` : html`
                    <div><dt>Agents:</dt><dd>${previewHost?.agents.length || 0}</dd></div>
                    <div><dt>Sessions:</dt><dd>${previewHost?.sessionSummaries.length || 0}</dd></div>
                  `}
                </dl>
                <button class="node-preview-action" type="button" @click=${() => this.openNode(previewNode)}>Control ${icon("arrow-right")}</button>
              </section>
            ` : nothing}
          </section>
          <section class="sessions-panel">
            <div class="section-head session-bar">
              <button class="catalog-toggle quiet small" @click=${() => { this.catalogOpen = !this.catalogOpen; this.requestUpdate(); }} aria-expanded=${this.catalogOpen} aria-controls="session-catalog">
                ${this.catalogOpen ? icon("chevron-down") : icon("chevron-right")} Sessions <span class="catalog-count">${filteredSessions.length}</span></button>
              <button class="small primary" @click=${() => this.toggleCreate()}
                ?disabled=${!activeHost || activeHost.state.status !== "connected"}>${icon("plus")} New session</button>
            </div>
            ${this.catalogOpen ? html`<div id="session-catalog" class="session-list">
              ${filteredSessions.slice(0, this.catalogLimit).map((s) => {
                const state = sessionState(s.summary, hosts.find((h) => h.id === s.hostId)?.state.status === "connected");
                return html`<button class="session-row ${selected?.hostId === s.hostId && selected?.uri === s.summary.resource ? "selected" : ""}" @click=${() => { this.catalogOpen = false; this.openSession(s.hostId, s.summary.resource); }}>
                  <span class="state-bullet ${state}"></span>
                  <span class="session-name"><strong>${s.summary.title || "Untitled session"}</strong><small>${s.hostLabel} / ${s.summary.provider} · ${tail(s.summary.workingDirectories?.[0])}</small></span>
                  <span class="session-status ${state}">${state}</span>
                </button>`;
              })}
              ${filteredSessions.length > this.catalogLimit ? html`<button class="show-more" @click=${() => { this.catalogLimit += 100; this.requestUpdate(); }}>${icon("chevron-down")} Show more · ${this.catalogLimit} / ${filteredSessions.length}</button>` : nothing}
              ${filteredSessions.length === 0 ? html`<p class="empty-row">${sessions.length ? "No matching sessions" : "No sessions"}</p>` : nothing}
            </div>` : nothing}
          </section>
        </section>

      </main>
      <dialog class="side-drawer" data-width="${this.rightDrawerWidth}px" aria-label="Fleet details"
        @close=${() => { this.drawerMode = ""; this.requestUpdate(); }}
        @click=${(event) => { if (event.target === event.currentTarget) this.closeDrawer(); }}>
        <div class="panel-resizer resizer-left" role="separator" aria-orientation="vertical" aria-label="Resize details drawer"
          @pointerdown=${(e) => this.beginPanelResize(e, "right")}
          @pointermove=${(e) => this.movePanelResize(e)}
          @pointerup=${(e) => this.endPanelResize(e)}
          @pointercancel=${(e) => this.endPanelResize(e)}
          @click=${(e) => e.stopPropagation()}></div>
        <div class="drawer-top"><span>${this.drawerMode === "create" ? "NEW SESSION" : this.drawerMode === "host" ? "HOST DETAILS" : this.drawerMode === "status" ? "STATUS" : "SESSION"}</span>
          <div class="drawer-tools">
            ${this.drawerMode === "session" && selected ? html`<button class="quiet small" type="button"
              @click=${() => this.refreshSelected()} ?disabled=${!live || this.busy}>${icon("refresh")} Refresh</button>` : nothing}
            <button class="quiet small drawer-close" type="button" aria-label="Close details" @click=${() => this.closeDrawer()}>${icon("close")}</button>
          </div></div>
        ${this.fleet.error && this.drawerMode ? html`<div class="alert" role="alert"><span>ERROR</span> ${safeError(this.fleet.error)}<button aria-label="Dismiss error" class="close" @click=${() => { this.fleet.error = ""; this.requestUpdate(); }}>${icon("close")}</button></div>` : nothing}
        ${this.drawerMode === "status" && !this.fleet.error ? html`<p class="drawer-empty">No current error.</p>` : nothing}
            ${this.drawerMode === "create" && activeHost ? html`
              <form class="create-form" @submit=${this.create} @change=${this.createFormChanged}>
                <label>Agent<select name="provider" required>
                  ${activeHost.agents.map((agent) => html`<option value="${agent.provider}">${agent.displayName}</option>`)}
                </select></label>
                <label>Workspace (file URI)<input name="directory" type="text" spellcheck="false" placeholder="file:///path/to/project" .value=${activeHost.defaultDirectory || ""}></label>
                <button class="primary" ?disabled=${this.busy || !activeHost.agents.length}>${icon("plus")} Create session</button>
                <label class="workspace-trust"><input type="checkbox" name="trustWorkspace">
                  <span><strong>Trust this workspace for this browser tab</strong>
                    <small>Allow this ACP agent to load repository instructions and hooks. Only select for projects you trust.</small>
                  </span>
                </label>
                ${activeHost.agents.length ? html`<div class="host-options">
                  <div class="section-label">HOST SESSION OPTIONS
                    <button class="quiet small" type="button" ?disabled=${this.busy}
                      @click=${(event) => this.resolveCreateConfig(event.currentTarget.form, !this.sessionConfig)}>${icon("refresh")} Refresh</button></div>
                  ${this.sessionConfig ? html`<div class="host-options-grid">
                    ${Object.entries(this.sessionConfig.schema?.properties || {}).map(([key, property]) =>
                      this.renderConfigField(key, property))}
                    ${!Object.keys(this.sessionConfig.schema?.properties || {}).length ? html`<p class="muted">No additional host options.</p>` : nothing}
                  </div>` : html`<p class="muted">Host options are not loaded. Refresh to request them.</p>`}
                </div>` : nothing}
                ${!activeHost.agents.length ? html`<p class="create-notice">The selected host currently advertises no agents.</p>` : nothing}
              </form>
            ` : nothing}
        ${this.drawerMode === "host" && activeHost ? html`
          <section class="host-detail" aria-label="Host details">
            <div class="inspector-title">
              <h2>${activeHost.label}</h2><span class="state-text ${activeHost.state.status}">${activeHost.state.status}</span>
            </div>
            <dl class="details">
              <div><dt>Protocol</dt><dd>${activeHost.protocolVersion ? "AHP " + activeHost.protocolVersion : "Pending"}</dd></div>
              <div><dt>Agents</dt><dd>${activeHost.agents.length}</dd></div>
              <div><dt>Sessions</dt><dd>${activeHost.sessionSummaries.length}</dd></div>
            </dl>
            ${activeHost.lastError ? html`<div class="attention-warning host-error" role="alert">${activeHost.lastError.message === "websocket failed to open" ? "WebSocket handshake failed. Check the host URL and access token." : safeError(activeHost.lastError.message)}</div>` : nothing}
            <div class="rail-actions">
              ${!this.fleet.eventStreamHealthy
                ? html`<button class="quiet small" @click=${() => location.reload()}>${icon("refresh")} Reload Relay</button>`
                : activeHost.state.status === "connected"
                ? html`<button class="quiet small" @click=${() => this.run(() => this.fleet.reconnect(activeHost.id))} ?disabled=${this.busy}>${icon("refresh")} Reconnect</button>`
                : html`<button class="quiet small" @click=${() => this.openConnectForm()} ?disabled=${this.busy}>${icon("arrow-right")} Update credentials</button>`}
              <button class="danger small" @click=${() => this.removeActiveHost(activeHost.id)} ?disabled=${this.busy}>${icon("close")} Remove host</button>
            </div>
            ${activeHost.agents.length ? html`<div class="host-agents"><div class="transcript-head">ADVERTISED AGENTS</div>
              ${activeHost.agents.map((agent) => html`<div class="host-agent"><strong>${agent.displayName}</strong><span>${agent.provider}</span></div>`)}</div>` : nothing}
          </section>` : nothing}
        ${this.drawerMode === "session" ? html`
        <aside class="inspector" aria-label="Session inspector">
          ${selected ? html`
            <div class="inspector-title">
              <span class="eyebrow">${this.fleet.currentHost()?.label || selected.hostId} / ${selectedSummary?.provider || detail?.provider || "SESSION"}</span>
              <h2>${selectedSummary?.title || detail?.title || (sessionLoadError ? "Session unavailable" : "Loading session")}</h2>
              <span class="state-text ${sessionLoadError ? "error" : sessionState(detail?.lifecycle === "failed" ? detail : selectedSummary || detail, live)}">${sessionLoadError ? "UNAVAILABLE" : sessionState(detail?.lifecycle === "failed" ? detail : selectedSummary || detail, live)}</span>
            </div>
            <div class="drawer-tabs" role="tablist" aria-label="Session view">
              ${[['chat', 'Conversation'], ['files', 'Files'], ['details', 'Info']].map(([mode, label]) => html`
                <button type="button" role="tab" id="${mode}-tab" class="drawer-tab ${this.sessionTab === mode ? 'active' : ''}"
                  aria-selected=${this.sessionTab === mode} aria-controls="${mode}-panel"
                  tabindex=${this.sessionTab === mode ? "0" : "-1"}
                  @keydown=${(event) => this.onSessionTabKey(event, mode)}
                  @click=${() => { this.sessionTab = mode; this.requestUpdate(); }}>${label}</button>`)}
            </div>
            ${sessionLoadError ? html`<div class="attention-warning" role="status">
              The host may have restored this session under a new ID. Inspect the host session catalog.
            </div>` : nothing}
            ${detail?.lifecycle === "failed" ? html`<div class="attention-warning" role="alert">
              ${safeError(detail.creationError?.message || "The host could not initialize this session.")}
            </div>` : nothing}
            ${!live ? html`<div class="attention-warning" role="status">Host disconnected. Published session summaries may be stale. Controls are unavailable.</div>` : nothing}
            ${this.sessionTab === "chat" ? html`<section id="chat-panel" class="drawer-panel" role="tabpanel" aria-labelledby="chat-tab" tabindex="0">
              ${attentionFor(detail).length ? html`<div class="attention-section">
              <div class="section-label">NEEDS ATTENTION <span>${attentionFor(detail).length}</span></div>
              ${attentionFor(detail).map((entry) => this.renderAttention(entry, live))}
            </div>` : nothing}
            ${detail?.chats?.length > 1 && selectableChats.length ? html`<label class="chat-select">CHAT
              <select .value=${this.fleet.slots.chat?.uri || ""} ?disabled=${!live || this.busy} @change=${(e) =>
                { this.explicitChatUri = e.target.value; this.historyOffset = 0; void this.run(() => this.fleet.selectChat(e.target.value)); }}>
                ${!selectableChats.some((entry) => entry.resource === this.fleet.slots.chat?.uri)
                  ? html`<option value="" disabled>Hidden chat selected</option>` : nothing}
                ${selectableChats.map((entry) => html`<option value="${entry.resource}">${entry.title || entry.resource}</option>`)}
              </select>
            </label>` : nothing}
            <div class="transcript-head"><span>CONVERSATION</span><span>${chat
              ? `${turns.length} loaded${chat.turnsNextCursor ? " · older on host" : ""}`
              : "Awaiting host"}</span></div>
            <div class="transcript" role="log" aria-live="off">
              ${chat ? html`
                ${historyStart > 0 || chat.turnsNextCursor ? html`
                  <button class="history-button quiet small" type="button" ?disabled=${!live || this.busy}
                    @click=${() => this.earlierTurns(chat)}>${icon("arrow-up")} Load earlier turns</button>` : nothing}
                ${this.historyOffset ? html`<div class="history-nav">
                  <button class="quiet small" type="button" @click=${() => { this.historyOffset = Math.max(0, this.historyOffset - 40); this.requestUpdate(); }}>${icon("arrow-up")} Newer turns</button>
                  <button class="quiet small" type="button" @click=${() => { this.historyOffset = 0; this.requestUpdate(); }}>${icon("arrow-down")} Latest</button>
                </div>` : nothing}
                ${turns.length ? html`<p class="history-position">${historyStart + 1}–${historyEnd} of ${turns.length} loaded turns</p>` : nothing}
                ${[...turns.slice(historyStart, historyEnd),
                  ...(this.historyOffset === 0 && chat.activeTurn ? [chat.activeTurn] : [])].map((turn) => this.renderTurn(turn))}
                ${!turns.length && !chat.activeTurn ? html`<p class="muted">No turns in this chat.</p>` : nothing}
              ` : html`<p class="muted">${sessionLoadError || chatLoadError ||
                (!live ? "Chat requires a live host connection." : detail && !selectableChats.length
                  ? "No visible chat for this session." : "Loading chat from the host…")}</p>`}
            </div>
            ${chat?.queuedMessages?.length ? html`<div class="queue-notice">${chat.queuedMessages.length} queued turn(s)</div>` : nothing}
            ${this.fleet.waiting.size ? html`<div class="queue-notice warning">Awaiting host echo for ${this.fleet.waiting.size} submission(s)</div>` : nothing}
            ${chat && !chatAllowsInput ? html`<div class="chat-restriction" role="status">
              ${promptPolicy.reason}
            </div>` : nothing}
            <form class="composer" @submit=${this.submitPrompt}>
              <label for="prompt">MESSAGE / QUEUED TURN</label>
              <textarea id="prompt" name="prompt" rows="4" placeholder="Describe the work to perform…" required ?disabled=${!live || !chat || !chatAllowsInput || this.busy}></textarea>
              ${models.length ? html`<label class="model-select">MODEL
                <select name="model" .value=${models.some((model) => model.id === this.modelId) ? this.modelId : ""}
                  ?disabled=${!live || !chatAllowsInput || this.busy} @change=${(event) => { this.modelId = event.target.value; this.requestUpdate(); }}>
                  <option value="">Host default</option>
                  ${models.map((model) => html`<option value="${model.id}" ?disabled=${model.policyState === "disabled"}>
                    ${model.name || model.id}${model.policyState === "unconfigured" ? " · Not configured" : ""}
                  </option>`)}
                </select>
              </label>` : nothing}
              <div class="composer-actions">
                <button class="quiet small" type="button" @click=${() => this.run(() => this.fleet.cancelTurn())} ?disabled=${!live || !chat?.activeTurn || this.busy}>${icon("close")} Cancel turn</button>
                <button class="primary small" type="submit" ?disabled=${!live || !chat || !chatAllowsInput || this.busy}>Queue turn ${icon("arrow-right")}</button>
              </div>
            </form>
            </section>` : nothing}
            ${this.sessionTab === "files" ? html`<section id="files-panel" class="drawer-panel" role="tabpanel" aria-labelledby="files-tab" tabindex="0">
            ${changesetCatalog.length ? html`<section class="changes-panel" aria-labelledby="changes-title">
              <div class="transcript-head"><span id="changes-title">FILES / CHANGES</span><span>${changes?.status || "Awaiting host"}</span></div>
              ${changesets.length > 1 ? html`<label class="changes-select">CHANGESET
                <select .value=${changeset?.uriTemplate || ""} ?disabled=${!live || this.busy}
                  @change=${(event) => { this.filePreview = null; void this.run(() => this.fleet.selectChangeset(event.target.value)); }}>
                  ${changesets.map((entry) => html`<option value="${entry.uriTemplate}">${entry.label}</option>`)}
                </select>
              </label>` : nothing}
              ${changesetTemplate.includes("{turnId}") && turnIds.length ? html`
                <label class="changes-select">TURN
                  <select .value=${this.fleet.changesetParams.turnId || turnIds.at(-1).id}
                    ?disabled=${!live || this.busy}
                    @change=${(event) => this.changeTurnForChangeset("turnId", event.target.value)}>
                    ${turnIds.map((turn, index) => html`<option value="${turn.id}">
                      ${index + 1} · ${short(turn.message?.text, 42)}
                    </option>`)}
                  </select>
                </label>` : nothing}
              ${changesetTemplate.includes("{originalTurnId}") && turnIds.length > 1 ? html`
                <div class="changeset-compare">
                  ${[["originalTurnId", "FROM", turnIds.at(-2).id],
                    ["modifiedTurnId", "TO", turnIds.at(-1).id]].map(([field, name, fallback]) => html`
                    <label class="changes-select">${name}
                      <select .value=${this.fleet.changesetParams[field] || fallback}
                        ?disabled=${!live || this.busy}
                        @change=${(event) => this.changeTurnForChangeset(field, event.target.value)}>
                        ${turnIds.map((turn, index) => html`<option value="${turn.id}">
                          ${index + 1} · ${short(turn.message?.text, 30)}
                        </option>`)}
                      </select>
                    </label>`)}
                </div>` : nothing}
              <div class="changes-body">
                <div class="changes-description">
                  <strong>${changeset?.label || (changesets.length ? "Loading changeset" : "No completed turns yet")}</strong>
                  ${changeset?.description ? html`<p>${changeset.description}</p>` : nothing}
                </div>
                ${!changesets.length
                  ? html`<p class="muted">Complete a turn to inspect its host-advertised changes.</p>`
                  : changesLoadError
                    ? html`<p class="change-error" role="alert">${changesLoadError}</p>`
                    : !changes
                      ? html`<p class="muted">${live ? "Subscribing to host changes…" : "Reconnect to inspect changes."}</p>`
                      : html`
                        ${changes.status === "error" ? html`<p class="change-error" role="alert">${changes.error?.message || "Changeset failed to load."}</p>` : nothing}
                        ${changes.status === "recomputing" ? html`<p class="muted">Recomputing. Displaying the previous result.</p>` : nothing}
                        <div class="changes-total">${changes.files.length} ${changes.files.length === 1 ? "file" : "files"}
                          ${canReviewChanges ? html`<span>${changes.files.filter((file) => file.reviewed).length} reviewed</span>` : nothing}
                        </div>
                        ${changes.files.map((file) => html`
                          <div class="change-file">
                            <div class="change-file-heading">
                              <strong title="${fileLabel(file)}">${tail(fileLabel(file))}</strong>
                              <span>${changeStats(file.edit?.diff)}</span>
                            </div>
                            <div class="change-file-meta"><span>${fileKind(file)}</span><span title="${fileLabel(file)}">${fileLabel(file)}</span></div>
                            ${fileKind(file) === "Renamed" ? html`<div class="change-file-from">From: ${file.edit.before.uri}</div>` : nothing}
                            <button class="change-inspect quiet small" type="button"
                              aria-expanded=${preview?.fileId === file.id}
                              ?disabled=${!live || this.busy || (this.filePreview?.loading && this.filePreview.fileId !== file.id)}
                              @click=${() => this.inspectFile(file.id)}>${preview?.fileId === file.id ? html`${icon("chevron-up")} Close preview` : html`${icon("chevron-down")} Inspect file`}</button>
                            ${preview?.fileId === file.id ? html`<div class="edit-preview" role="region" aria-label="File content preview">
                              ${preview.loading ? html`<p class="muted">Reading file content from the host…</p>`
                                : preview.error ? html`<p class="change-error" role="alert">${preview.error}</p>`
                                : html`<div class="edit-sides">
                                    <section><h3>Before</h3>${preview.before === null
                                      ? html`<p>No before content supplied.</p>`
                                      : html`<pre>${preview.before}</pre>`}</section>
                                    <section><h3>After</h3>${preview.after === null
                                      ? html`<p>No after content supplied.</p>`
                                      : html`<pre>${preview.after}</pre>`}</section>
                                  </div>`}
                            </div>` : nothing}
                            ${canReviewChanges ? html`<label class="review-file">
                              <input type="checkbox" .checked=${!!file.reviewed} ?disabled=${!live || this.busy}
                                @change=${(event) => this.run(() => this.fleet.reviewFile(file.id, event.target.checked))}>
                              <span>Reviewed</span>
                            </label>` : nothing}
                          </div>`)}
                        ${changes.status === "ready" && !changes.files.length ? html`<p class="muted">Host reports no changed files in this changeset.</p>` : nothing}
                      `}
              </div>
            </section>` : nothing}
              ${!changesetCatalog.length ? html`<p class="drawer-empty">Host advertises no file changes.</p>` : nothing}
            </section>` : nothing}
            ${this.sessionTab === "details" ? html`<section id="details-panel" class="drawer-panel" role="tabpanel" aria-labelledby="details-tab" tabindex="0">
            <dl class="details">
              <div><dt>Session</dt><dd title="${selected.uri}">${tail(selected.uri)}</dd></div>
              <div><dt>Workspace</dt><dd title="${detail?.workingDirectories?.[0] || selectedSummary?.workingDirectories?.[0] || ""}">${tail(detail?.workingDirectories?.[0] || selectedSummary?.workingDirectories?.[0])}</dd></div>
              <div><dt>Last activity</dt><dd>${when(selectedSummary?.modifiedAt)}</dd></div>
              <div><dt>Changes</dt><dd>${summaryStats(selectedSummary?.changes)}</dd></div>
            </dl>
            <div class="inspector-footer"><button class="danger small" @click=${() => { if (window.confirm("Dispose this AHP session? This cannot be undone.")) void this.run(async () => { await this.fleet.disposeSession(); this.closeDrawer(); }); }} ?disabled=${!live || this.busy || !!sessionLoadError}>${icon("close")} Dispose session</button></div>
            </section>` : nothing}
          ` : html`
            <div class="inspector-empty"><span class="eyebrow">NO SELECTION</span><h2>Select a session</h2><p>Select a session in the topology or catalog to see authoritative state, conversation, and approval requests.</p></div>
          `}
        </aside>
        ` : nothing}
      </dialog>
      <dialog class="shortcuts-dialog" aria-labelledby="shortcuts-title">
        <div class="shortcuts-head"><div><span class="eyebrow">OPERATOR REFERENCE</span><h2 id="shortcuts-title">Keyboard controls</h2></div>
          <button class="close preview-close" type="button" aria-label="Close keyboard controls" @click=${() => this.querySelector(".shortcuts-dialog")?.close()}>
            <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M2 2 L10 10 M10 2 L2 10" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>
          </button></div>
        <dl class="shortcuts-grid">
          <div><dt><kbd>/</kbd></dt><dd>Focus fleet search</dd></div>
          <div><dt><kbd>?</kbd></dt><dd>Open this reference</dd></div>
          <div><dt><kbd>↑</kbd> <kbd>↓</kbd> <kbd>←</kbd> <kbd>→</kbd></dt><dd>Move between topology nodes</dd></div>
          <div><dt><kbd>Home</kbd> <kbd>End</kbd></dt><dd>First host or last visible session</dd></div>
          <div><dt><kbd>Enter</kbd></dt><dd>Open controls for the selected topology node</dd></div>
          <div><dt><kbd>Esc</kbd></dt><dd>Clear a topology selection or search, or close this dialog</dd></div>
        </dl>
      </dialog>
    `;
  }

  renderTurn(turn) {
    const parts = turn.responseParts || [];
    const noResponse = turn.state === "complete" && !parts.length;
    return html`<div class="turn">
      <div class="turn-meta"><span class="turn-kind">PROMPT</span><span>${when(turn.startedAt)}</span></div>
      <p class="turn-prompt">${turn.message?.text || "—"}</p>
      ${parts.map((part) =>
        part.kind === "markdown" ? html`<pre class="response-text">${part.content}</pre>` :
        part.kind === "toolCall" ? html`<div class="tool-event"><span>TOOL</span><b>${part.toolCall?.displayName || part.toolCall?.toolName}</b><small>${part.toolCall?.status}</small></div>` :
        part.kind === "error" ? html`<div class="alert">${part.error?.message || "Agent error"}</div>` :
        part.kind === "inputRequest" ? html`<div class="tool-event">Agent requested input · ${part.response || "pending"}</div>` : nothing)}
      <div class="turn-outcome">${noResponse ? "Host reports complete · no response recorded" : turn.state || "In progress"}${turn.usage?.totalTokens ? " · " + turn.usage.totalTokens + " tokens" : ""}</div>
    </div>`;
  }

  renderAttention(entry, live) {
    if (entry.kind === "toolConfirmation") {
      const call = entry.toolCall;
      const options = call.options || [];
      return html`<div class="attention-item">
        <strong>${labelText(call.confirmationTitle) || call.displayName || "Tool confirmation"}</strong>
        <p>${call.intention || labelText(call.invocationMessage) || call.toolName || "Review this tool request"}</p>
        <details class="tool-detail"><summary>Tool parameters</summary>
          <pre class="tool-input">${JSON.stringify(call.toolInput ?? {}, null, 2)}</pre>
        </details>
        <div class="attention-actions">
          ${call.status === "pending-result-confirmation"
            ? html`
              <button class="primary small" ?disabled=${!live || this.busy}
                @click=${() => this.confirm(entry, true)}>${icon("check")} Accept result</button>
              <button class="quiet small" ?disabled=${!live || this.busy}
                @click=${() => this.confirm(entry, false)}>${icon("close")} Reject result</button>
            ` : options.length
              ? options.filter((option) => ["approve", "deny"].includes(option.kind)).map((option) => html`
                  <button class="${option.kind === "approve" ? "primary" : "quiet"} small"
                    ?disabled=${!live || this.busy}
                    @click=${() => this.confirm(entry, option.kind === "approve", option.id)}>${option.kind === "approve" ? icon("check") : icon("close")} ${option.label}</button>`)
              : html`
                  <button class="primary small" ?disabled=${!live || this.busy}
                    @click=${() => this.confirm(entry, true)}>${icon("check")} Approve once</button>
                  <button class="quiet small" ?disabled=${!live || this.busy}
                    @click=${() => this.confirm(entry, false)}>${icon("close")} Deny</button>`}
        </div>
      </div>`;
    }
    if (entry.kind === "chatInput") {
      return html`<form class="attention-item" @submit=${(event) => this.answer(event, entry)}>
        <strong>${entry.request.message || "Agent requires input"}</strong>
        ${safeLink(entry.request.url) ? html`<a href="${safeLink(entry.request.url)}" target="_blank" rel="noopener noreferrer">Review requested link ↗</a>` : nothing}
        ${(entry.request.questions || []).map((q) => html`<fieldset class="question"><legend>${q.title || q.message}</legend>
          ${q.kind === "boolean" ? html`<select name="${q.id}" .value=${q.defaultValue === undefined ? "" : String(q.defaultValue)}
            ?required=${q.required}><option value="" disabled>Choose an answer</option><option value="true">Yes</option><option value="false">No</option></select>` :
            q.kind === "single-select" || q.kind === "multi-select" ? html`
              <select name="${q.id}" ?multiple=${q.kind === "multi-select"} ?required=${q.required && !q.allowFreeformInput}>
                ${q.kind === "single-select" ? html`<option value="">Choose an option</option>` : nothing}
                ${(q.options || []).map((choice) => html`<option value="${choice.id}">${choice.label}</option>`)}
              </select>
              ${q.allowFreeformInput ? html`<label class="freeform-label">Other response
                <input name="${q.id}:freeform" type="text" autocomplete="off" placeholder="Enter a response">
              </label>` : nothing}` :
            html`<input name="${q.id}" type="${q.kind === "integer" || q.kind === "number" ? "number" : "text"}" ?required=${q.required}
              step="${q.kind === "number" ? "any" : "1"}" min=${q.kind === "number" || q.kind === "integer" ? q.min ?? nothing : nothing}
              max=${q.kind === "number" || q.kind === "integer" ? q.max ?? nothing : nothing}
              minlength=${q.kind === "text" ? q.min ?? nothing : nothing}
              maxlength=${q.kind === "text" ? q.max ?? nothing : nothing}
              .value=${q.defaultValue == null ? "" : String(q.defaultValue)} placeholder="${q.message}">`}
        </fieldset>`)}
        <div class="attention-actions">
          <button type="submit" class="primary small" ?disabled=${!live || this.busy}>${icon("check")} Answer</button>
          <button type="button" class="quiet small" ?disabled=${!live || this.busy} @click=${() => this.run(() => this.fleet.answerInput(entry, "decline"))}>${icon("close")} Decline</button>
        </div>
      </form>`;
    }
    return html`<div class="attention-item"><strong>${entry.kind}</strong><p>Resolve the authentication request in the agent host.</p></div>`;
  }
}

customElements.define("relay-fleet", RelayFleet);
