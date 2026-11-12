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
import { Scan } from "lucide-react";
import { topologyFocusId } from "./helpers";
import { buildGraph } from "../../graph.mjs";
import {
  edgeGradientCoords,
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
  onConnect?: () => void;
};
const shortText = (value: string, length = 32) =>
  String(value || "—").length > length
    ? String(value).slice(0, length - 1) + "…"
    : String(value || "—");
const nodeTitleLines = (node: Node) => {
  const limit = node.kind === "session" ? 10 : 11;
  const words = String(node.title || "—").trim().split(/\s+/).filter(Boolean);
  const lines: string[] = [];
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
  const folded = lines.slice(0, 2).map((line) => shortText(line, limit));
  if (lines.length > 2 && !folded[1].endsWith("…")) {
    folded[1] += "…";
  }
  return folded;
};
const nodeTitleFontSize = (node: Node, lines: string[]) => {
  const originalLength = String(node.title || "—").trim().length;
  if (lines.length === 1 && originalLength <= 10) return 9.0;
  if (lines.length === 1 && originalLength <= 20) return 6.5;
  return 6.0;
};
const nodeTitleSizeClass = (fontSize: number) =>
  fontSize >= 9.0 ? "title-short" : fontSize >= 6.5 ? "title-medium" : "title-long";
const nodeTitleLayout = (node: Node, lines: string[]) => {
  const fontSize = nodeTitleFontSize(node, lines);
  const topPad = lines.length > 1 ? 2.2 : (fontSize >= 9.0 ? 3.0 : 4.5);
  const lineStep = lines.length > 1 ? 6.2 : 0;
  const firstBaseline = node.y + topPad + fontSize;
  return { fontSize, firstBaseline, lineStep, sizeClass: nodeTitleSizeClass(fontSize) };
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

export default function Topology({
  hosts,
  sessions,
  query,
  selectedId,
  onPreview,
  onControl,
  signal,
  onConnect,
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
  const [zoom, setZoom] = useState(1);
  const focusId = topologyFocusId(graph.nodes, focused, selectedId);
  const chosen = graph.nodes.find((n: Node) => n.id === selectedId);
  const pathFocusId = hoverNode || selectedId || "";
  const relatedIds = useMemo(
    () => relatedTopologyIds(graph, pathFocusId),
    [graph, pathFocusId],
  );
  function zoomBy(delta: number) {
    setZoom((prev) => Math.max(0.4, Math.min(2.4, Math.round((prev + delta) * 100) / 100)));
  }
  function fitToCanvas() {
    const view = viewport.current;
    if (!view || !graph.nodes.length) return;
    const minX = Math.min(...graph.nodes.map((node) => node.x));
    const maxX = Math.max(...graph.nodes.map((node) => node.x + node.w));
    const minY = Math.min(...graph.nodes.map((node) => node.y));
    const maxY = Math.max(...graph.nodes.map((node) => node.y + node.h));
    const boxW = Math.max(1, maxX - minX + 64);
    const boxH = Math.max(1, maxY - minY + 64);
    const availW = Math.max(120, view.clientWidth - 48);
    const availH = Math.max(120, view.clientHeight - 96);
    const nextZoom = Math.max(
      0.4,
      Math.min(2, Math.round(Math.min(availW / boxW, availH / boxH) * 100) / 100),
    );
    setZoom(nextZoom);
    requestAnimationFrame(() => {
      const v = viewport.current;
      if (!v) return;
      const centerX = ((minX + maxX) / 2) * nextZoom;
      const centerY = ((minY + maxY) / 2) * nextZoom;
      v.scrollLeft = Math.max(0, centerX - v.clientWidth / 2);
      v.scrollTop = Math.max(0, centerY - (v.clientHeight - 56) / 2);
      setOffset({ x: v.scrollLeft, y: v.scrollTop });
    });
  }
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
      const firstHost =
        graph.nodes.find((n) => n.kind === "host") || graph.nodes[0];
      const targetX =
        view.clientWidth < 500 && firstHost
          ? firstHost.x + firstHost.w / 2 - view.clientWidth / 2
          : (minX + maxX - view.clientWidth) / 2;
      view.scrollLeft = Math.max(
        0,
        Math.min(maxScrollX, targetX),
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
        zoom,
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
      onWheel={(e) => {
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          zoomBy(e.deltaY < 0 ? 0.12 : -0.12);
        }
      }}
      onScroll={(e) =>
        setOffset({
          x: e.currentTarget.scrollLeft,
          y: e.currentTarget.scrollTop,
        })
      }
    >
      <div className="topology-zoom-controls" role="group" aria-label="Canvas zoom controls">
        <button
          type="button"
          className="topology-zoom-btn"
          aria-label="Zoom out"
          onClick={() => zoomBy(-0.15)}
        >
          −
        </button>
        <span className="topology-zoom-readout" aria-live="polite">
          {Math.round(zoom * 100)}%
        </span>
        <button
          type="button"
          className="topology-zoom-btn"
          aria-label="Zoom in"
          onClick={() => zoomBy(0.15)}
        >
          +
        </button>
        <button
          type="button"
          className="topology-fit-btn"
          title="Fit to canvas"
          aria-label="Fit to canvas"
          onClick={fitToCanvas}
        >
          <Scan className="topology-fit-icon" aria-hidden="true" />
        </button>
      </div>
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
          {!hosts.length && onConnect && (
            <button
              type="button"
              className="action-primary empty-connect-btn"
              onClick={onConnect}
            >
              + Connect a host
            </button>
          )}
        </div>
      ) : (
        <div
          className="topology-plane"
          style={{
            width: (graph.width * zoom) / 16 + "rem",
            height: (graph.height * zoom) / 16 + "rem",
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
                  strokeWidth="1.0"
                />
              </pattern>
              <linearGradient
                id={gridId + "-signal"}
                gradientUnits="userSpaceOnUse"
                x1={Math.round(graph.width * 0.15)}
                y1={Math.round(graph.height * 0.15)}
                x2={Math.round(graph.width * 0.85)}
                y2={Math.round(graph.height * 0.85)}
              >
                <stop offset="0%" stopColor="#c4ff00" />
                <stop offset="25%" stopColor="#96ff20" />
                <stop offset="50%" stopColor="#50ff60" />
                <stop offset="75%" stopColor="#20ffa0" />
                <stop offset="100%" stopColor="#00ffc4" />
              </linearGradient>
              <linearGradient
                id={gridId + "-working-border"}
                x1="0%"
                y1="0%"
                x2="100%"
                y2="100%"
              >
                <stop offset="0%" stopColor="#ffffff" />
                <stop offset="25%" stopColor="#a0a0a0" />
                <stop offset="50%" stopColor="#303030" />
                <stop offset="75%" stopColor="#a0a0a0" />
                <stop offset="100%" stopColor="#ffffff" />
                <animateTransform
                  attributeName="gradientTransform"
                  type="rotate"
                  from="0 0.5 0.5"
                  to="360 0.5 0.5"
                  dur="2.4s"
                  repeatCount="indefinite"
                />
              </linearGradient>
              {graph.edges.map((edge: any, edgeIndex: number) => {
                const pts = edgeGradientCoords(edge.d);
                return (
                  <linearGradient
                    key={edge.from + edge.to + "-grad"}
                    id={gridId + "-edge-" + edgeIndex}
                    gradientUnits="userSpaceOnUse"
                    x1={pts.x1}
                    y1={pts.y1}
                    x2={pts.x2}
                    y2={pts.y2}
                  >
                    <stop offset="0%" stopColor="#c4ff00" />
                    <stop offset="25%" stopColor="#96ff20" />
                    <stop offset="50%" stopColor="#50ff60" />
                    <stop offset="75%" stopColor="#20ffa0" />
                    <stop offset="100%" stopColor="#00ffc4" />
                  </linearGradient>
                );
              })}
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
            {graph.edges.map((edge: any, edgeIndex: number) => (
              <path
                key={edge.from + edge.to}
                stroke={
                  edge.stale
                    ? undefined
                    : "url(#" + gridId + "-edge-" + edgeIndex + ")"
                }
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
              const lines = nodeTitleLines(node);
              const layout = nodeTitleLayout(node, lines);
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
                  {node.status === "working" && (
                    <rect
                      className="node-working-border"
                      x={node.x}
                      y={node.y}
                      width={node.w}
                      height={node.h}
                      pathLength={100}
                      stroke={`url(#${gridId}-working-border)`}
                    />
                  )}
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
                    cy={node.y + 6}
                    r="2"
                    aria-hidden="true"
                  />
                  {details.map(([label], i) => (
                    <g
                      key={label}
                      className={"node-tool" + (facet[node.id] === i ? " pinned" : "")}
                      role="button"
                      tabIndex={focusId === node.id ? 0 : -1}
                      aria-label={label.toLowerCase() + " for " + node.title}
                      aria-pressed={facet[node.id] === i}
                      onPointerEnter={() => setHover({ id: node.id, index: i })}
                      onPointerLeave={() => setHover(null)}
                      onClick={(e) => {
                        e.stopPropagation();
                        setFacet((prev) => {
                          const next = { ...prev };
                          if (next[node.id] === i) delete next[node.id];
                          else next[node.id] = i;
                          return next;
                        });
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          e.stopPropagation();
                          setFacet((prev) => {
                            const next = { ...prev };
                            if (next[node.id] === i) delete next[node.id];
                            else next[node.id] = i;
                            return next;
                          });
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
                          cx={node.x + 8.5}
                          cy={node.y + 5.0 + i * ((node.h - 10) / 2)}
                          r="2"
                        />
                      ) : i === 1 ? (
                        <path
                          className="node-tool-glyph"
                          d={
                            "M" +
                            (node.x + 6.25) +
                            " " +
                            (node.y + 5.0 + i * ((node.h - 10) / 2) - 1.55) +
                            "h1.5l.75.75h2.25v2.35h-4.5z"
                          }
                        />
                      ) : (
                        <path
                          className="node-tool-glyph"
                          d={
                            "M" +
                            (node.x + 5.9) +
                            " " +
                            (node.y + 5.0 + i * ((node.h - 10) / 2) - 2) +
                            "h5.2m-5.2 2h5.2m-5.2 2h3.5"
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
                        y={node.y + 8.2}
                      >
                        {detail[0]}
                      </text>
                      <text
                        className="node-detail-value"
                        x={node.x + 22}
                        y={node.y + 14.4}
                      >
                        {clip(detail[1], 16)}
                      </text>
                    </>
                  ) : (
                    <text
                      className={`node-title ${layout.sizeClass}`}
                      x={node.x + 22}
                      y={layout.firstBaseline}
                    >
                      {lines.map((line, lineIdx) => (
                        <tspan
                          key={lineIdx}
                          x={node.x + 22}
                          dy={lineIdx === 0 ? 0 : layout.lineStep}
                        >
                          {line}
                        </tspan>
                      ))}
                    </text>
                  )}
                  {[0, 1, 2, 3].map((i) => (
                    <rect
                      key={i}
                      className={"node-signal " + (i < level ? "on" : "")}
                      x={node.x + 22 + i * 12}
                      y={node.y + node.h - 5.8}
                      width="9.5"
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
