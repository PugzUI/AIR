import { sessionState } from "./fleet.mjs";

export const GRAPH_CARD_SIZE = { w: 74, h: 32 };
// Keep the plane bounded but spacious enough to pan past every edge on a
// normal desktop viewport. Nodes and routes use this same coordinate frame.
export const GRAPH_PADDING = { x: 520, y: 320 };
const lane = {
  minWidth: 424,
  gapX: 24,
  gapY: 24,
  providerColumns: 3,
  hostGridGap: 16,
  componentGap: 16,
  providerSessionGap: 24,
  sessionGapX: 12,
  sessionGapY: 12,
};

/** @param {number} count */
const sessionCluster = (count) => {
  const columns = Math.min(3, Math.max(1, count));
  const rows = Math.max(1, Math.ceil(count / columns));
  return {
    columns,
    width: columns * GRAPH_CARD_SIZE.w + (columns - 1) * lane.sessionGapX,
    height: rows * GRAPH_CARD_SIZE.h + (rows - 1) * lane.sessionGapY,
  };
};

/** @param {any[]} providers */
const providerGrid = (providers) => {
  if (!providers.length) {
    return {
      columns: 0, cellWidth: 0, width: 0, rowHeights: [], height: 0, cells: [],
    };
  }
  const columns = Math.min(lane.providerColumns, Math.max(1, providers.length));
  const rows = Math.max(1, Math.ceil(providers.length / columns));
  const cells = providers.map((provider) => ({
    ...provider,
    cluster: sessionCluster(provider.rows.length),
  }));
  const cellWidth = Math.max(
    GRAPH_CARD_SIZE.w,
    ...cells.map(({ cluster }) => cluster.width),
  );
  const rowHeights = Array.from({ length: rows }, (_, row) =>
    Math.max(
      GRAPH_CARD_SIZE.h + lane.providerSessionGap + GRAPH_CARD_SIZE.h,
      ...cells
        .slice(row * columns, (row + 1) * columns)
        .map(({ cluster }) =>
          GRAPH_CARD_SIZE.h + lane.providerSessionGap + Math.max(GRAPH_CARD_SIZE.h, cluster.height)),
    ));
  return {
    columns,
    cellWidth,
    width: columns * cellWidth + (columns - 1) * lane.gapX,
    rowHeights,
    height: rowHeights.reduce((sum, height) => sum + height, 0) +
      (rows - 1) * lane.gapY,
    cells,
  };
};

const match = /** @param {any} terms
 * @param {any} fields */
              (terms, fields) => terms.every(/** @param {any} term */ (term) =>
  fields.some(/** @param {any} field */ (field) => String(field ?? "").toLowerCase().includes(term)));

/** @param {number} x1
 * @param {number} y1
 * @param {number} x2
 * @param {number} y2
 * @param {number} bendY */
function elbow(x1, y1, x2, y2, bendY = (y1 + y2) / 2) {
  const bend = Math.round(bendY);
  return `M ${Math.round(x1)} ${Math.round(y1)} V ${bend} H ${Math.round(x2)} V ${Math.round(y2)}`;
}

/** @param {number} hostCenterX
 * @param {number} hostFacingY
 * @param {{x: number, y: number, w: number, h: number}} provider
 * @param {"up" | "down"} direction
 * @param {number | null} branchX */
function hostProviderElbow(hostCenterX, hostFacingY, provider, direction, branchX = null) {
  const providerCenterX = provider.x + provider.w / 2;
  const edgeX = providerCenterX < hostCenterX ? provider.x + provider.w : provider.x;
  const providerFacingY = direction === "up"
    ? provider.y + provider.h + 18
    : provider.y - 18;
  if (branchX !== null) {
    return `M ${Math.round(hostCenterX)} ${Math.round(hostFacingY)} H ${Math.round(branchX)} V ${Math.round(providerFacingY)} H ${Math.round(edgeX)} V ${Math.round(provider.y + provider.h / 2)}`;
  }
  return elbow(hostCenterX, hostFacingY, edgeX, provider.y + provider.h / 2,
    providerFacingY);
}

/** @param {{x: number, y: number, w: number, h: number}} provider
 * @param {{x: number, y: number, w: number, h: number}} session
 * @param {"up" | "down"} direction */
function providerSessionElbow(provider, session, direction) {
  const providerFacingY = direction === "up" ? provider.y : provider.y + provider.h;
  const sessionFacingY = direction === "up" ? session.y + session.h : session.y;
  const bendY = direction === "up" ? sessionFacingY + 10 : sessionFacingY - 10;
  return elbow(provider.x + provider.w / 2, providerFacingY,
    session.x + session.w / 2, sessionFacingY, bendY);
}

// Pure projection of the AHP host and session snapshots. Session providers may
// outlive the host's current agent advertisement; those sessions stay visible.
// Each host is a compact direction-aware component: a balanced provider grid
// with bounded three-column child clusters, packed on one centered spine.
/** @param {any} hosts
 * @param {any} sessions
 * @param {any} query
 * @param {any} limit */
export function buildGraph(hosts, sessions, query = "", limit = 16) {
  const terms = String(query).trim().toLowerCase().split(/\s+/).filter(Boolean);
  const hostAgents = new Map(hosts.map(/** @param {any} host */ (host) => [
    host.id, new Map((host.agents || []).map(/** @param {any} agent */ (agent) => [agent.provider, agent.displayName])),
  ]));
  const visible = sessions.filter(/** @param {any} arg0 */ ({ hostId, hostLabel, summary }) => match(terms, [
    hostId, hostLabel, summary.resource, summary.title, summary.provider,
    hostAgents.get(hostId)?.get(summary.provider), summary.activity,
    ...(summary.workingDirectories || []),
  ]));
  // Keep exceptional sessions in the capped topology so aggregate attention
  // and working signals always have a reachable record. Preserve source order
  // within each priority group; the full catalog remains authoritative.
  const priority = /** @param {any} row */ (row) => {
    const state = sessionState(row.summary, true);
    return ["attention", "working", "error", "connecting", "reconnecting"].includes(state) ? 0 : 1;
  };
  const shown = visible
    .map(/** @param {any} row
 * @param {any} index */
         (row, index) => ({ row, index, priority: priority(row) }))
    .sort(/** @param {any} a
 * @param {any} b */
          (a, b) => a.priority - b.priority || a.index - b.index)
    .slice(0, Math.max(0, limit))
    .map(/** @param {any} arg0 */ ({ row }) => row);

  // Phase one: retain the source grouping and compute each host component's
  // intrinsic grid dimensions before assigning any coordinates. This makes
  // the shared column width deterministic and keeps later hosts from shifting
  // when an earlier host happens to have a larger child cluster.
  const hostRecords = [];
  for (const host of hosts) {
    const hostMatches = terms.length > 0 && match(terms, [
      host.id, host.label, host.state.status,
    ]);
    const matchingAgents = (host.agents || []).filter(/** @param {any} agent */ (agent) =>
      match(terms, [agent.provider, agent.displayName]));
    const visibleOnHost = visible.filter(/** @param {any} row */ (row) => row.hostId === host.id);
    if (terms.length && !hostMatches && !visibleOnHost.length && !matchingAgents.length) continue;

    const owned = shown.filter(/** @param {any} row */ (row) => row.hostId === host.id);
    const grouped = new Map();
    for (const row of owned) {
      const provider = row.summary.provider || "";
      if (!grouped.has(provider)) grouped.set(provider, []);
      grouped.get(provider).push(row);
    }
    const advertised = (host.agents || []).filter(/** @param {any} agent */ (agent) => !terms.length ||
      hostMatches || visibleOnHost.some(/** @param {any} row */ (row) => row.summary.provider === agent.provider) ||
      match(terms, [agent.provider, agent.displayName]));
    const providers = advertised.map(/** @param {any} agent */ (agent) => ({ ...agent, advertised: true }));
    for (const provider of grouped.keys()) {
      if (!providers.some(/** @param {any} agent */ (agent) => agent.provider === provider)) {
        providers.push({ provider, displayName: provider || "Unknown agent", advertised: false });
      }
    }

    const grid = providerGrid(providers.map(/** @param {any} agent */ (agent) => ({
      agent,
      rows: grouped.get(agent.provider) || [],
    })));
    const blockHeight = GRAPH_CARD_SIZE.h +
      (providers.length ? lane.hostGridGap + grid.height : 0);
    hostRecords.push({
      host, visibleOnHost, providers, grid, blockHeight,
      componentWidth: Math.max(lane.minWidth, grid.width),
    });
  }

  const componentWidth = Math.max(
    lane.minWidth,
    ...hostRecords.map(({ componentWidth: width }) => width),
  );
  const nodes = [];
  const edges = [];
  let blockY = GRAPH_PADDING.y;

  // Phase two: emit the host components in one centered column. Even slots
  // fold upward (sessions, providers, host); odd slots remain top-down.
  for (const [hostIndex, record] of hostRecords.entries()) {
    const { host, visibleOnHost, providers, grid } = record;
    const direction = hostIndex % 2 === 0 ? "up" : "down";
    const componentX = GRAPH_PADDING.x;
    const gridX = componentX + (componentWidth - grid.width) / 2;
    const hostCenterX = componentX + componentWidth / 2;
    const hostY = direction === "up"
      ? blockY + grid.height + lane.hostGridGap
      : blockY;
    const hostNode = {
      id: host.id, kind: "host", hostId: host.id,
      x: Math.round(hostCenterX - GRAPH_CARD_SIZE.w / 2), y: Math.round(hostY),
      w: GRAPH_CARD_SIZE.w, h: GRAPH_CARD_SIZE.h,
      title: host.label, subtitle: host.state.status, status: host.state.status,
      agentCount: host.agents?.length || 0, sessionCount: visibleOnHost.length,
    };
    nodes.push(hostNode);
    const gridY = direction === "up"
      ? blockY
      : hostNode.y + hostNode.h + lane.hostGridGap;
    const visualRowHeights = direction === "up"
      ? [...grid.rowHeights].reverse()
      : grid.rowHeights;
    for (let index = 0; index < grid.cells.length; index++) {
      const { agent, rows, cluster } = grid.cells[index];
      const row = Math.floor(index / grid.columns);
      const column = index % grid.columns;
      const cellX = gridX + column * (grid.cellWidth + lane.gapX);
      const visualRow = direction === "up" ? grid.rowHeights.length - row - 1 : row;
      const cellY = gridY + visualRowHeights
        .slice(0, visualRow)
        .reduce((sum, height) => sum + height + lane.gapY, 0);
      const clusterX = Math.round(cellX + (grid.cellWidth - cluster.width) / 2);
      const providerX = Math.round(cellX + (grid.cellWidth - GRAPH_CARD_SIZE.w) / 2);
      const rowHeight = visualRowHeights[visualRow];
      const providerY = direction === "up"
        ? cellY + rowHeight - GRAPH_CARD_SIZE.h
        : cellY;
      const providerId = host.id + "\0agent\0" + agent.provider;
      const providerNode = {
        id: providerId, kind: "provider", hostId: host.id,
        x: providerX, y: Math.round(providerY),
        w: GRAPH_CARD_SIZE.w, h: GRAPH_CARD_SIZE.h,
        title: agent.displayName || agent.provider || "Unknown agent",
        subtitle: agent.advertised ? (agent.provider || "Unspecified") : "Session provider · not advertised",
        status: host.state.status, advertised: agent.advertised,
        provider: agent.provider || "", sessionCount: rows.length,
      };
      nodes.push(providerNode);
      edges.push({
        hostId: host.id, from: hostNode.id, to: providerId,
        stale: host.state.status !== "connected",
        d: hostProviderElbow(
          hostCenterX,
          direction === "up" ? hostNode.y : hostNode.y + hostNode.h,
          providerNode,
          direction,
          row > 0 ? componentX - lane.gapX : null,
        ),
      });

      for (const [sessionIndex, source] of rows.entries()) {
        const sessionColumn = sessionIndex % cluster.columns;
        const sessionRow = Math.floor(sessionIndex / cluster.columns);
        const x = Math.round(clusterX + sessionColumn * (GRAPH_CARD_SIZE.w + lane.sessionGapX));
        const y = direction === "up"
          ? Math.round(providerNode.y - lane.providerSessionGap - cluster.height +
            sessionRow * (GRAPH_CARD_SIZE.h + lane.sessionGapY))
          : Math.round(providerNode.y + providerNode.h + lane.providerSessionGap +
            sessionRow * (GRAPH_CARD_SIZE.h + lane.sessionGapY));
        const sessionNode = {
          id: host.id + "\0" + source.summary.resource,
          kind: "session", hostId: host.id, uri: source.summary.resource,
          x, y, w: GRAPH_CARD_SIZE.w, h: GRAPH_CARD_SIZE.h,
          title: source.summary.title || source.summary.provider || "Untitled session",
          subtitle: sessionState(source.summary, host.state.status === "connected"),
          status: sessionState(source.summary, host.state.status === "connected"),
          provider: source.summary.provider || "",
          activity: source.summary.activity || "",
          workspace: source.summary.workingDirectories?.[0] || "",
          chat: source.summary.defaultChat || source.summary.chats?.[0]?.resource,
        };
        nodes.push(sessionNode);
        edges.push({
          hostId: host.id, from: providerId, to: sessionNode.id,
          sessionUri: sessionNode.uri, chat: sessionNode.chat,
          stale: host.state.status !== "connected",
          d: providerSessionElbow(providerNode, sessionNode, direction),
        });
      }
    }
    blockY += record.blockHeight + lane.componentGap;
  }

  const contentBottom = hostRecords.length
    ? blockY - lane.componentGap
    : GRAPH_PADDING.y;
  const width = Math.max(680, componentWidth + GRAPH_PADDING.x * 2);
  return {
    nodes, edges, width, height: Math.max(360, contentBottom + GRAPH_PADDING.y),
    padding: GRAPH_PADDING,
    initialScroll: {
      x: Math.max(0, GRAPH_PADDING.x - 48),
      y: Math.max(0, GRAPH_PADDING.y - 48),
    },
    shown: shown.length, total: visible.length,
  };
}
