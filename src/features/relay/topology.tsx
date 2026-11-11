import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import { topologyFocusId } from "./helpers";
import { buildGraph } from "../../graph.mjs";
import {
  paintTopology,
  relatedTopologyIds,
  releaseTopology,
} from "../../../ui/topology-fx.mjs";

type Node = {
  id: string;
  kind: string;
  hostId: string;
  uri?: string;
  title: string;
  subtitle: string;
  status: string;
  x: number;
  y: number;
  w: number;
  h: number;
  provider?: string;
  workspace?: string;
  agentCount?: number;
  sessionCount?: number;
  advertised?: boolean;
};
type Props = {
  hosts: any[];
  sessions: any[];
  query: string;
  selectedId?: string;
  onPreview: (node: any) => void;
  onControl: (node: any) => void;
  signal?: any;
};
const clip = (text: string, length = 17) =>
  text.length > length ? text.slice(0, length - 1) + "…" : text;
const tail = (value?: string) =>
  value?.split("/").filter(Boolean).at(-1) || "Not reported";
const networkPalette = [
  "var(--c-data-teal)",
  "var(--c-data-gold)",
];
const networkColor = (node: Node) => {
  const identity = node.provider
    ? "provider:" + node.provider
    : "host:" + node.hostId;
  let hash = 0;
  for (const character of identity) hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  return networkPalette[hash % networkPalette.length];
};
const activeStates = [
  "working",
  "attention",
  "error",
  "failed",
  "connecting",
  "reconnecting",
  "offline",
  "disconnected",
  "stale",
];

export default function Topology({
  hosts,
  sessions,
  query,
  selectedId,
  onPreview,
  onControl,
  signal,
}: Props) {
  const graph = useMemo(
    () => buildGraph(hosts, sessions, query),
    [hosts, sessions, query],
  ) as {
    nodes: Node[];
    edges: any[];
    width: number;
    height: number;
    total: number;
    shown: number;
    initialScroll?: { x: number; y: number };
  };
  const gridId = "grid-" + useId().replaceAll(":", "");
  const viewport = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const nodeRefs = useRef(new Map<string, SVGGElement>());
  const initialFrame = useRef(false);
  const drag = useRef<{
    x: number;
    y: number;
    left: number;
    top: number;
    moved: boolean;
  } | null>(null);
  const [focused, setFocused] = useState("");
  const [facet, setFacet] = useState<Record<string, number>>({});
  const [hover, setHover] = useState<{ id: string; index: number } | null>(
    null,
  );
  const [hoverNode, setHoverNode] = useState("");
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const focusId = topologyFocusId(graph.nodes, focused, selectedId);
  const chosen = graph.nodes.find((n: Node) => n.id === selectedId);
  const pathFocusId = hoverNode || selectedId || "";
  const relatedIds = useMemo(
    () => relatedTopologyIds(graph, pathFocusId),
    [graph, pathFocusId],
  );
  useEffect(() => {
    const view = viewport.current;
    if (!view || initialFrame.current || !graph.nodes.length) return;
    const frame = requestAnimationFrame(() => {
      if (initialFrame.current) return;
      const minX = Math.min(...graph.nodes.map((node) => node.x));
      const maxX = Math.max(...graph.nodes.map((node) => node.x + node.w));
      const minY = Math.min(...graph.nodes.map((node) => node.y));
      const maxY = Math.max(...graph.nodes.map((node) => node.y + node.h));
      const maxScrollX = Math.max(0, view.scrollWidth - view.clientWidth);
      const maxScrollY = Math.max(0, view.scrollHeight - view.clientHeight);
      view.scrollLeft = Math.max(
        0,
        Math.min(maxScrollX, (minX + maxX - view.clientWidth) / 2),
      );
      view.scrollTop = Math.max(
        0,
        Math.min(maxScrollY, (minY + maxY - view.clientHeight) / 2),
      );
      setOffset({ x: view.scrollLeft, y: view.scrollTop });
      initialFrame.current = true;
    });
    return () => cancelAnimationFrame(frame);
  }, [graph]);
  useEffect(() => {
    const element = canvas.current;
    const view = viewport.current;
    if (!element || !view) return;
    const paint = () =>
      paintTopology(
        element,
        graph,
        chosen?.kind === "session"
          ? { hostId: chosen.hostId, uri: chosen.uri }
          : null,
        signal,
        signal?.uri,
        view.scrollTop,
        view.clientHeight,
        selectedId || "",
        pathFocusId,
      );
    paint();
    const observer = new ResizeObserver(paint);
    observer.observe(view);
    const motion = matchMedia("(prefers-reduced-motion: reduce)");
    motion.addEventListener("change", paint);
    return () => {
      observer.disconnect();
      motion.removeEventListener("change", paint);
      releaseTopology(element);
    };
  }, [graph, selectedId, signal, offset.y, chosen, pathFocusId]);
  function key(event: KeyboardEvent<SVGGElement>, node: Node) {
    if (event.key === "Enter") {
      event.preventDefault();
      onControl(node);
      return;
    }
    if (event.key === " ") {
      event.preventDefault();
      onPreview(node);
      return;
    }
    if (
      ![
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowDown",
        "Home",
        "End",
      ].includes(event.key)
    )
      return;
    event.preventDefault();
    let next =
      event.key === "Home"
        ? graph.nodes[0]
        : event.key === "End"
          ? graph.nodes.at(-1)
          : null;
    if (!next) {
      const horizontal =
        event.key === "ArrowLeft" || event.key === "ArrowRight";
      const direction =
        event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
      next = graph.nodes
        .filter(
          (candidate: Node) =>
            candidate.id !== node.id &&
            (horizontal ? candidate.x - node.x : candidate.y - node.y) *
              direction >
              0,
        )
        .sort((a: Node, b: Node) => {
          const score = (n: Node) =>
            horizontal
              ? Math.abs(n.x - node.x) + Math.abs(n.y - node.y) * 2
              : Math.abs(n.y - node.y) + Math.abs(n.x - node.x) * 2;
          return score(a) - score(b);
        })[0];
    }
    if (next) {
      setFocused(next.id);
      nodeRefs.current.get(next.id)?.focus();
      nodeRefs.current
        .get(next.id)
        ?.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
  }
  function startPan(event: PointerEvent<HTMLDivElement>) {
    if (
      event.button !== 0 ||
      (event.target as Element).closest("[role=button]")
    )
      return;
    const view = event.currentTarget;
    drag.current = {
      x: event.clientX,
      y: event.clientY,
      left: view.scrollLeft,
      top: view.scrollTop,
      moved: false,
    };
    view.setPointerCapture(event.pointerId);
  }
  function pan(event: PointerEvent<HTMLDivElement>) {
    const initial = drag.current;
    if (!initial) return;
    const dx = event.clientX - initial.x,
      dy = event.clientY - initial.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) initial.moved = true;
    event.currentTarget.scrollLeft = initial.left - dx;
    event.currentTarget.scrollTop = initial.top - dy;
  }
  return (
    <div
      ref={viewport}
      className="topology-viewport"
      role="region"
      aria-label="Pannable fleet topology"
      tabIndex={0}
      onPointerDown={startPan}
      onPointerMove={pan}
      onPointerUp={() => {
        drag.current = null;
      }}
      onPointerCancel={() => {
        drag.current = null;
      }}
      onScroll={(e) =>
        setOffset({
          x: e.currentTarget.scrollLeft,
          y: e.currentTarget.scrollTop,
        })
      }
    >
      <p className="sr-only" id={gridId + "-help"}>
        Use arrow keys between nodes, Space to preview, Enter to open controls.
        Drag empty space or scroll to pan. The Sessions list offers the same
        records.
      </p>
      {!graph.nodes.length ? (
        <div className="topology-empty">
          <span className="empty-cross" aria-hidden="true">
            +
          </span>
          <h2>
            {hosts.length ? "No matching nodes" : "Your fleet starts here"}
          </h2>
          <p>
            {hosts.length
              ? "Try another host, agent, or session name."
              : "Connect an AHP host to map its agents and sessions."}
          </p>
        </div>
      ) : (
        <div
          className="topology-plane"
          style={{
            width: graph.width / 16 + "rem",
            height: graph.height / 16 + "rem",
          }}
        >
          <canvas
            ref={canvas}
            className="topology-lightfield"
            aria-hidden="true"
            style={{ top: offset.y }}
          />
          <svg
            className="topology-map"
            viewBox={"0 0 " + graph.width + " " + graph.height}
            role="group"
            aria-label="Host to agent to session topology"
            aria-describedby={gridId + "-help"}
          >
            <defs>
              <pattern
                id={gridId}
                width="20"
                height="20"
                patternUnits="userSpaceOnUse"
              >
                <path
                  d="M20 0H0V20"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="0.5"
                />
              </pattern>
              <pattern
                id={gridId + "-major"}
                width="80"
                height="80"
                patternUnits="userSpaceOnUse"
              >
                <path
                  d="M80 0H0V80"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="0.8"
                />
              </pattern>
            </defs>
            <rect
              width="100%"
              height="100%"
              fill={"url(#" + gridId + ")"}
              className="topology-grid"
            />
            <rect
              width="100%"
              height="100%"
              fill={"url(#" + gridId + "-major)"}
              className="topology-grid-major"
            />
            {graph.edges.map((edge: any) => (
              <path
                key={edge.from + edge.to}
                className={
                  "topology-edge " +
                  (edge.stale ? "stale " : "") +
                  (pathFocusId
                    ? relatedIds.has(edge.from) && relatedIds.has(edge.to)
                      ? "related"
                      : "dimmed"
                    : "")
                }
                d={edge.d}
              />
            ))}
            {graph.nodes.map((node: Node) => {
              const details =
                node.kind === "session"
                  ? [
                      ["STATE", node.subtitle],
                      ["WORKSPACE", tail(node.workspace)],
                      ["AGENT", node.provider || "Unknown"],
                    ]
                  : node.kind === "provider"
                    ? [
                        ["STATE", node.subtitle],
                        ["SESSIONS", String(node.sessionCount || 0)],
                        ["PROVIDER", node.provider || node.title],
                      ]
                    : [
                        ["STATE", node.subtitle],
                        ["AGENTS", String(node.agentCount || 0)],
                        ["SESSIONS", String(node.sessionCount || 0)],
                      ];
              const tool = hover?.id === node.id ? hover.index : facet[node.id];
              const detail = tool !== undefined ? details[tool] : null;
              const level = [
                "working",
                "attention",
                "error",
                "failed",
              ].includes(node.status)
                ? 4
                : ["connecting", "reconnecting"].includes(node.status)
                  ? 3
                  : node.status === "connected"
                    ? 2
                    : node.status === "idle"
                      ? 1
                      : 0;
              const nodeDimmed = Boolean(pathFocusId) && !relatedIds.has(node.id);
              return (
                <g
                  key={node.id}
                  ref={(el) => {
                    if (el) nodeRefs.current.set(node.id, el);
                    else nodeRefs.current.delete(node.id);
                  }}
                  className={
                    "topology-node " +
                    node.kind +
                    " " +
                    node.status +
                    (selectedId === node.id ? " selected" : "") +
                    (nodeDimmed ? " dimmed" : "")
                  }
                  style={{ "--network-color": networkColor(node) } as CSSProperties}
                  role="button"
                  tabIndex={focusId === node.id ? 0 : -1}
                  aria-label={
                    node.kind + ": " + node.title + ", " + node.subtitle
                  }
                  aria-current={selectedId === node.id ? "true" : undefined}
                  onFocus={() => {
                    setFocused(node.id);
                    setHoverNode(node.id);
                  }}
                  onPointerEnter={() => setHoverNode(node.id)}
                  onPointerLeave={() =>
                    setHoverNode((current) => (current === node.id ? "" : current))
                  }
                  onKeyDown={(e) => key(e, node)}
                  onClick={() => onPreview(node)}
                >
                  <rect
                    className="node-frame"
                    x={node.x}
                    y={node.y}
                    width={node.w}
                    height={node.h}
                  />
                  <rect
                    className="node-rail"
                    x={node.x + 1}
                    y={node.y + 1}
                    width="15"
                    height={node.h - 2}
                  />
                  <line
                    className="node-divider"
                    x1={node.x + 16}
                    y1={node.y + 1}
                    x2={node.x + 16}
                    y2={node.y + node.h - 1}
                  />
                  <circle
                    className="node-status-glyph"
                    cx={node.x + node.w - 6}
                    cy={node.y + 7}
                    r="2"
                    aria-hidden="true"
                  />
                  {details.map(([label], i) => (
                    <g
                      key={label}
                      role="button"
                      tabIndex={focusId === node.id ? 0 : -1}
                      aria-label={label.toLowerCase() + " for " + node.title}
                      aria-pressed={facet[node.id] === i}
                      onPointerEnter={() => setHover({ id: node.id, index: i })}
                      onPointerLeave={() => setHover(null)}
                      onClick={(e) => {
                        e.stopPropagation();
                        setFacet({ ...facet, [node.id]: i });
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          e.stopPropagation();
                          setFacet({ ...facet, [node.id]: i });
                        }
                      }}
                    >
                      <rect
                        className="node-tool-hit"
                        x={node.x + 1}
                        y={node.y + (i * node.h) / 3}
                        width="15"
                        height={node.h / 3}
                      />
                      {i === 0 ? (
                        <circle
                          className="node-tool-glyph"
                          cx={node.x + 8}
                          cy={node.y + ((i + 0.5) * node.h) / 3}
                          r="2"
                        />
                      ) : i === 1 ? (
                        <path
                          className="node-tool-glyph"
                          d={
                            "M" +
                            (node.x + 5) +
                            " " +
                            (node.y + ((i + 0.5) * node.h) / 3 - 2) +
                            "h6v4h-6z"
                          }
                        />
                      ) : (
                        <path
                          className="node-tool-glyph"
                          d={
                            "M" +
                            (node.x + 5) +
                            " " +
                            (node.y + ((i + 0.5) * node.h) / 3 - 2) +
                            "h6m-6 2h6m-6 2h4"
                          }
                        />
                      )}
                    </g>
                  ))}
                  {detail ? (
                    <>
                      <text
                        className="node-detail-label"
                        x={node.x + 22}
                        y={node.y + 11}
                      >
                        {detail[0]}
                      </text>
                      <text
                        className="node-detail-value"
                        x={node.x + 22}
                        y={node.y + 22}
                      >
                        {clip(detail[1], 16)}
                      </text>
                    </>
                  ) : (
                    <>
                      <text
                        className="node-title"
                        x={node.x + 22}
                        y={node.y + 14}
                      >
                        {clip(node.title, node.kind === "session" ? 16 : 17)}
                      </text>
                      {activeStates.includes(node.status) && (
                        <text
                          className="node-state"
                          textAnchor="end"
                          x={node.x + node.w - 5}
                          y={node.y + node.h - 5}
                        >
                          {node.status.toUpperCase()}
                        </text>
                      )}
                    </>
                  )}
                  {[0, 1, 2, 3].map((i) => (
                    <rect
                      key={i}
                      className={"node-signal " + (i < level ? "on" : "")}
                      x={node.x + 22 + i * 11}
                      y={node.y + node.h - 6}
                      width="8"
                      height="2"
                    />
                  ))}
                </g>
              );
            })}
          </svg>
        </div>
      )}
      {graph.total > graph.shown && (
        <p className="topology-overflow">
          Showing {graph.shown} of {graph.total} sessions. Open Sessions for the
          full list.
        </p>
      )}
    </div>
  );
}
