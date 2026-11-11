import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronLeft, ChevronRight, Plus, Search, Server, X } from "lucide-react";
import { Button } from "@/components/nexvyn/button";
import { Fleet, sessionState } from "../../fleet.mjs";
import Topology from "./topology";
import { CreateSession } from "./create-session";
import { SessionInspector } from "./session";
import { safeError, tail } from "./helpers";

export default function RelayApp() {
  const [fleet, setFleet] = useState<any>(null);
  const [, render] = useState(0);
  const [activeHostId, setActiveHostId] = useState("");
  const [expanded, setExpanded] = useState(false);
  const [connectOpen, setConnectOpen] = useState(false);
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [node, setNode] = useState<any>(null);
  const [drawer, setDrawer] = useState("");
  const [provider, setProvider] = useState("");
  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState("");
  const [shortcuts, setShortcuts] = useState(false);
  const drawerRef = useRef<HTMLDialogElement>(null);
  const shortcutsRef = useRef<HTMLDialogElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const busyRef = useRef(false);
  const mounted = useRef(false);
  const lastFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    mounted.current = true;
    const instance = new Fleet(() => {
      if (mounted.current) render((n) => n + 1);
    });
    setFleet(instance);
    return () => {
      mounted.current = false;
      void instance.shutdown();
    };
  }, []);
  useEffect(() => {
    const dialog = drawerRef.current;
    if (!dialog) return;
    if (drawer && !dialog.open) {
      lastFocus.current = document.activeElement as HTMLElement;
      dialog.showModal();
    }
    if (!drawer && dialog.open) dialog.close();
  }, [drawer]);
  useEffect(() => {
    const dialog = shortcutsRef.current;
    if (!dialog) return;
    if (shortcuts && !dialog.open) dialog.showModal();
    if (!shortcuts && dialog.open) dialog.close();
  }, [shortcuts]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (event.key === "Escape" && target === searchRef.current) {
        event.preventDefault();
        setQuery("");
        return;
      }
      if (event.key === "Escape" && !drawer && !shortcuts) {
        if (connectOpen) {
          setConnectOpen(false);
          return;
        }
        if (expanded) {
          setExpanded(false);
          return;
        }
        if (node) {
          setNode(null);
          return;
        }
      }
      if (
        target?.matches('input, textarea, select, [contenteditable="true"]') ||
        event.ctrlKey ||
        event.metaKey ||
        event.altKey
      )
        return;
      if (event.key === "/" && !drawer && !shortcuts) {
        event.preventDefault();
        searchRef.current?.focus();
      } else if (event.key === "?" && !drawer) {
        event.preventDefault();
        setShortcuts(true);
      } else if (event.key === "Escape" && !drawer && !shortcuts) {
        if (node) setNode(null);
        else if (query) setQuery("");
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [drawer, shortcuts, node, query, connectOpen, expanded]);

  const run = useCallback(
    async (fn: () => any) => {
      if (busyRef.current) return false;
      busyRef.current = true;
      setBusy(true);
      setLocalError("");
      if (fleet) fleet.error = "";
      try {
        await fn();
        return true;
      } catch (error) {
        if (mounted.current) setLocalError(safeError(error));
        return false;
      } finally {
        busyRef.current = false;
        if (mounted.current) {
          setBusy(false);
          render((n) => n + 1);
        }
      }
    },
    [fleet],
  );
  useEffect(() => {
    if (!fleet) return;
    const recover = () => {
      if (
        document.visibilityState === "visible" &&
        fleet.selected &&
        fleet.eventStreamHealthy &&
        fleet.currentHost()?.state.status === "connected" &&
        !busyRef.current
      )
        void run(() => fleet.refreshSelection());
    };
    document.addEventListener("visibilitychange", recover);
    return () => document.removeEventListener("visibilitychange", recover);
  }, [fleet, run]);
  const hosts: any[] = fleet
    ? fleet.eventStreamHealthy
      ? fleet.hosts()
      : fleet.hosts().map((host: any) => ({
          ...host,
          state: { ...host.state, status: "stale" },
        }))
    : [];
  const sessions: any[] = fleet?.sessions() || [];
  const activeHost = hosts.find((host) => host.id === activeHostId) || hosts[0] || null;
  const selected = fleet?.selected;
  const selectedSummary = sessions.find(
    (row) => row.hostId === selected?.hostId && row.summary.resource === selected?.uri,
  )?.summary;
  const selectedHost = hosts.find((host) => host.id === selected?.hostId);
  const online = hosts.filter((host) => host.state.status === "connected").length;
  const working = sessions.filter(
    (row) =>
      sessionState(
        row.summary,
        hosts.find((host) => host.id === row.hostId)?.state.status === "connected",
      ) === "working",
  ).length;
  const attention = sessions.filter(
    (row) =>
      sessionState(
        row.summary,
        hosts.find((host) => host.id === row.hostId)?.state.status === "connected",
      ) === "attention",
  ).length;
  const filtered = sessions.filter((row) => {
    const haystack = [
      row.hostLabel,
      row.summary.title,
      row.summary.provider,
      ...(row.summary.workingDirectories || []),
    ]
      .join(" ")
      .toLowerCase();
    return query
      .toLowerCase()
      .split(/\s+/)
      .filter(Boolean)
      .every((term) => haystack.includes(term));
  });
  const displayError = localError || fleet?.error || "";
  const liveActive =
    !!fleet?.eventStreamHealthy && activeHost?.state.status === "connected";
  function openConnect() {
    setNode(null);
    setDrawer("");
    setCatalogOpen(false);
    setActiveHostId("");
    setConnectOpen(true);
  }
  function openHost(host: any) {
    setActiveHostId(host.id);
    setNode(null);
    setDrawer("host");
  }
  function openSession(hostId: string, uri: string) {
    setActiveHostId(hostId);
    setDrawer("session");
    if (fleet && (fleet.selected?.hostId !== hostId || fleet.selected?.uri !== uri))
      void run(() => fleet.select({ hostId, uri }));
  }
  function openNode(item: any) {
    setNode(item);
    setActiveHostId(item.hostId);
    if (item.kind === "session") openSession(item.hostId, item.uri);
    else if (item.kind === "provider") {
      setProvider(item.provider);
      setDrawer("provider");
    } else setDrawer("host");
  }
  function closeDrawer() {
    setDrawer("");
  }
  return (
    <div className="relay-app" data-theme="live">
      <a className="skip-link" href="#main">
        Skip to fleet
      </a>
      <header className="relay-header">
        <div className="relay-brand">
          <svg viewBox="0 0 26 26" aria-hidden="true">
            <path
              d="M2 3H17V10H8V23H24M17 3H24M8 16H19"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            />
          </svg>
          <div>
            <strong>Relay</strong>
            <span> / Fleet</span>
          </div>
        </div>
        <div className="relay-header-stats">
          <span className="relay-connection-status">
            {online} / {hosts.length} hosts online
          </span>
          {displayError && (
            <Button variant="ghost" onClick={() => setDrawer("status")}>
              Issue
            </Button>
          )}
        </div>
      </header>
      {displayError && !drawer && (
        <div className="error-message" role="alert">
          {safeError(displayError)}{" "}
          <Button
            variant="ghost"
            onClick={() => {
              setLocalError("");
              if (fleet) {
                fleet.error = "";
                render((n) => n + 1);
              }
            }}
          >
            Dismiss
          </Button>
        </div>
      )}
      <div className="relay-layout">
        <aside className={`host-rail${expanded ? " host-rail-expanded" : ""}`}>
          <div className="host-rail-head">
            <Button
              variant="ghost"
              className="sidebar-toggle"
              aria-label={expanded ? "Collapse hosts" : "Expand hosts"}
              aria-expanded={expanded}
              size="icon"
              onClick={() => setExpanded(!expanded)}
            >
              {expanded ? <ChevronLeft /> : <ChevronRight />}
            </Button>
            {expanded && <strong>Hosts</strong>}
          </div>
          {expanded ? (
            <div className="host-rail-list">
              {hosts.length > 0 ? (
                <nav aria-label="Hosts">
                  <ul className="host-navigation">
                    {hosts.map((host) => (
                      <li key={host.id}>
                        <button
                          type="button"
                          className={`host-nav-item${host.id === activeHost?.id ? " is-selected" : ""}`}
                          aria-current={host.id === activeHost?.id ? "true" : undefined}
                          onClick={() => openHost(host)}
                        >
                          <span className="host-nav-name">{host.label}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </nav>
              ) : (
                <p className="muted">No hosts connected</p>
              )}
              <Button variant="ghost" onClick={openConnect}>
                <Plus /> Add host
              </Button>
            </div>
          ) : (
            <div className="host-rail-list">
              {hosts.map((host) => (
                <button
                  key={host.id}
                  type="button"
                  className={`host-nav-item${host.id === activeHost?.id ? " is-selected" : ""}`}
                  aria-current={host.id === activeHost?.id ? "true" : undefined}
                  aria-label={`Host ${host.label}, ${host.state.status}`}
                  onClick={() => openHost(host)}
                >
                  <Server />
                </button>
              ))}
            </div>
          )}
        </aside>
        <main className="relay-main" id="main">
          <div className="relay-toolbar">
            <Button variant="ghost" className="action-primary" onClick={openConnect}>
              <Plus /> Add host
            </Button>
            <div className="relay-legend">
              <span>
                <span className="status-dot working" /> {working ? working + " ACTIVE" : "ACTIVE"}
              </span>
              <span>
                <span className="status-dot attention" />{" "}
                {attention ? attention + " ATTENTION" : "ATTENTION"}
              </span>
              <span>
                <span className="status-dot idle" /> IDLE
              </span>
              <Button
                variant="ghost"
                className="topology-keys"
                size="sm"
                onClick={() => setShortcuts(true)}
                aria-label="Keyboard shortcuts"
              >
                Keys <kbd aria-hidden="true">?</kbd>
              </Button>
            </div>
          </div>
          {connectOpen && (
            <form
              className="connection-tray"
              onSubmit={(event) => {
                event.preventDefault();
                const form = event.currentTarget;
                const data = new FormData(form);
                void run(async () => {
                  await fleet.addHost({
                    label: String(data.get("label") || ""),
                    endpoint: String(data.get("endpoint") || ""),
                    token: String(data.get("token") || ""),
                  });
                  setActiveHostId(
                    String(data.get("label") || "")
                      .trim()
                      .toLowerCase()
                      .replace(/[^a-z0-9_-]+/g, "-"),
                  );
                  form.reset();
                  setConnectOpen(false);
                });
              }}
            >
              <div className="action-row connection-heading">
                <strong>Connect a host</strong>
                <Button
                  type="button"
                  variant="ghost"
                  aria-label="Close connection form"
                  onClick={() => setConnectOpen(false)}
                >
                  <X />
                </Button>
              </div>
              <label>
                Label
                <input className="field" name="label" autoComplete="off" required />
              </label>
              <label>
                WebSocket endpoint
                <input
                  className="field"
                  name="endpoint"
                  placeholder="wss://host.example"
                  autoComplete="off"
                  required
                />
              </label>
              <label>
                Access token
                <input className="field" name="token" type="password" autoComplete="off" required />
              </label>
              <p className="muted connection-note">
                Credentials remain in memory for this tab and are never saved.
              </p>
              <Button
                type="submit"
                variant="ghost"
                className="action-primary"
                disabled={!fleet || busy}
              >
                Connect host
              </Button>
            </form>
          )}
          {node && (
            <section className="node-preview" aria-label="Selected node preview">
              <div className="node-preview-identity">
                <h2>{node.title}</h2>
                <span className="node-preview-kind">{node.kind?.toUpperCase()}</span>
              </div>
              <dl className="node-preview-details">
                <div>
                  <dt>Status</dt>
                  <dd>{node.status || node.subtitle || "—"}</dd>
                </div>
                {node.kind !== "host" && (
                  <div>
                    <dt>Host</dt>
                    <dd>{hosts.find((host) => host.id === node.hostId)?.label || node.hostId}</dd>
                  </div>
                )}
                {node.kind === "session" && (
                  <>
                    <div>
                      <dt>Agent</dt>
                      <dd>{node.provider || "—"}</dd>
                    </div>
                    <div>
                      <dt>Workspace</dt>
                      <dd>{tail(node.workspace)}</dd>
                    </div>
                  </>
                )}
                {node.kind !== "session" && (
                  <div>
                    <dt>Sessions</dt>
                    <dd>{node.sessionCount ?? 0}</dd>
                  </div>
                )}
              </dl>
              <div className="node-preview-actions">
                <Button
                  variant="ghost"
                  className="action-primary preview-action"
                  size="sm"
                  onClick={() => openNode(node)}
                >
                  Control →
                </Button>
                <Button
                  variant="ghost"
                  className="preview-close"
                  aria-label="Close preview"
                  onClick={() => setNode(null)}
                  size="icon-sm"
                >
                  <X />
                </Button>
              </div>
            </section>
          )}
          <section className="topology-stage" aria-label="Fleet topology">
            <div className="topology-search">
              <Search aria-hidden="true" />
              <input
                ref={searchRef}
                className="field"
                aria-label="Search fleet"
                placeholder="Search fleet /"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
              {query && (
                <Button variant="ghost" aria-label="Clear search" onClick={() => setQuery("")}>
                  <X />
                </Button>
              )}
            </div>
            <Topology
              hosts={hosts}
              sessions={sessions}
              query={query}
              selectedId={node?.id}
              onPreview={setNode}
              onControl={openNode}
              signal={fleet?.lastSignal}
            />
            {!hosts.length && (
              <div className="empty-state">
                <h2>No connected hosts</h2>
                <p>Connect an AHP host to see its agents and sessions.</p>
                <Button variant="ghost" className="action-primary" onClick={openConnect}>
                  <Plus /> Connect a host
                </Button>
              </div>
            )}
          </section>
          <section className="sessions-catalog">
            <div className="sessions-heading">
              <Button
                variant="ghost"
                aria-expanded={catalogOpen}
                aria-controls="session-catalog"
                onClick={() => setCatalogOpen(!catalogOpen)}
              >
                {catalogOpen ? <ChevronDown /> : <ChevronRight />} Sessions{" "}
                <span>{filtered.length}</span>
              </Button>
              <Button
                variant="ghost"
                className="action-primary"
                disabled={!liveActive || busy}
                onClick={() => {
                  setProvider("");
                  setDrawer("create");
                }}
              >
                <Plus /> New session
              </Button>
            </div>
            {catalogOpen && (
              <div id="session-catalog" className="sessions-list">
                {filtered.map((row) => (
                  <button
                    type="button"
                    key={`${row.hostId}\0${row.summary.resource}`}
                    onClick={() => {
                      setCatalogOpen(false);
                      setNode({
                        id: `${row.hostId}\0${row.summary.resource}`,
                        kind: "session",
                        hostId: row.hostId,
                        uri: row.summary.resource,
                        title: row.summary.title || "Untitled session",
                        provider: row.summary.provider,
                        workspace: row.summary.workingDirectories?.[0],
                      });
                      openSession(row.hostId, row.summary.resource);
                    }}
                  >
                    <span className="status-dot" />
                    <span>
                      <strong>{row.summary.title || "Untitled session"}</strong>
                      <small>
                        {row.hostLabel} / {row.summary.provider} ·{" "}
                        {tail(row.summary.workingDirectories?.[0])}
                      </small>
                    </span>
                    <span>
                      {sessionState(
                        row.summary,
                        hosts.find((host) => host.id === row.hostId)?.state.status ===
                          "connected",
                      )}
                    </span>
                  </button>
                ))}
                {!filtered.length && (
                  <p className="muted">
                    {sessions.length ? "No matching sessions" : "No sessions"}
                  </p>
                )}
              </div>
            )}
          </section>
        </main>
      </div>
      <dialog
        className="relay-drawer"
        ref={drawerRef}
        aria-label="Fleet details"
        onClose={() => {
          setDrawer("");
          lastFocus.current?.focus();
        }}
        onClick={(event) => {
          if (event.target === event.currentTarget) closeDrawer();
        }}
      >
        <div className="drawer-header">
          <strong>
            {drawer === "create"
              ? "NEW SESSION"
              : drawer === "host"
                ? "HOST DETAILS"
                : drawer === "provider"
                  ? "PROVIDER"
                  : drawer === "status"
                    ? "STATUS"
                    : "SESSION"}
          </strong>
          <Button variant="ghost" aria-label="Close details" onClick={closeDrawer} size="icon">
            <X />
          </Button>
        </div>
        <div className="drawer-content">
          {displayError && (
            <div className="error-message" role="alert">
              {safeError(displayError)}{" "}
              <Button
                variant="ghost"
                onClick={() => {
                  setLocalError("");
                  if (fleet) {
                    fleet.error = "";
                    render((n) => n + 1);
                  }
                }}
              >
                Dismiss
              </Button>
            </div>
          )}
          {drawer === "status" && !displayError && <p className="muted">No current error.</p>}
          {drawer === "host" && activeHost && (
            <section className="panel-section">
              <h2>{activeHost.label}</h2>
              <p>{activeHost.state.status}</p>
              <dl>
                <dt>Protocol</dt>
                <dd>
                  {activeHost.protocolVersion ? `AHP ${activeHost.protocolVersion}` : "Pending"}
                </dd>
                <dt>Agents</dt>
                <dd>{activeHost.agents?.length || 0}</dd>
                <dt>Sessions</dt>
                <dd>{sessions.filter((row) => row.hostId === activeHost.id).length}</dd>
              </dl>
              {activeHost.lastError && (
                <p role="alert">
                  {safeError(
                    activeHost.lastError.message === "websocket failed to open"
                      ? "WebSocket handshake failed. Check the host URL and access token."
                      : activeHost.lastError.message,
                  )}
                </p>
              )}
              <div className="action-row">
                {!fleet?.eventStreamHealthy ? (
                  <Button variant="ghost" onClick={() => window.location.reload()}>
                    Reload Relay
                  </Button>
                ) : activeHost.state.status === "connected" ? (
                  <Button
                    variant="ghost"
                    disabled={busy}
                    onClick={() => void run(() => fleet.reconnect(activeHost.id))}
                  >
                    Reconnect
                  </Button>
                ) : (
                  <Button variant="ghost" onClick={openConnect}>
                    Update credentials
                  </Button>
                )}
                <Button
                  variant="ghost"
                  destructive
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      await fleet.removeHost(activeHost.id);
                      setActiveHostId("");
                      closeDrawer();
                    })
                  }
                >
                  Remove host
                </Button>
              </div>
              <h3>Advertised agents</h3>
              {(activeHost.agents || []).map((agent: any) => (
                <p key={agent.provider}>
                  <strong>{agent.displayName || agent.provider}</strong> · {agent.provider}
                </p>
              ))}
            </section>
          )}
          {drawer === "provider" && activeHost && (
            <section className="panel-section">
              <h2>{node?.title || provider}</h2>
              <p>{provider}</p>
              <p>
                {node?.advertised === false
                  ? "This provider is not currently advertised by the host."
                  : "Advertised by host"}
              </p>
              <p>
                {
                  sessions.filter(
                    (row) => row.hostId === activeHost.id && row.summary.provider === provider,
                  ).length
                }{" "}
                sessions
              </p>
              <Button
                variant="ghost"
                className="action-primary"
                disabled={!liveActive || node?.advertised === false || busy}
                onClick={() => setDrawer("create")}
              >
                New session
              </Button>
            </section>
          )}
          {drawer === "create" && activeHost && fleet && (
            <CreateSession
              key={activeHost.id}
              fleet={fleet}
              host={activeHost}
              busy={busy}
              run={run}
              initialProvider={provider}
              created={() => {
                const current = fleet.selected;
                if (current) {
                  setNode({
                    id: `${current.hostId}\0${current.uri}`,
                    kind: "session",
                    hostId: current.hostId,
                    uri: current.uri,
                  });
                  setDrawer("session");
                }
              }}
            />
          )}
          {drawer === "session" && selected && (
            <SessionInspector
              key={`live:${selected.hostId}:${selected.uri}`}
              fleet={fleet}
              selected={selected}
              summary={selectedSummary}
              host={selectedHost}
              busy={busy}
              run={run}
            />
          )}
          {drawer === "session" && !selected && (
            <p className="muted">Select a session to inspect it.</p>
          )}
        </div>
      </dialog>
      <dialog
        className="shortcut-dialog"
        ref={shortcutsRef}
        aria-labelledby="shortcuts-title"
        onClose={() => setShortcuts(false)}
      >
        <div className="drawer-header">
          <h2 id="shortcuts-title">Keyboard controls</h2>
          <Button
            variant="ghost"
            aria-label="Close keyboard controls"
            onClick={() => setShortcuts(false)}
            size="icon"
          >
            <X />
          </Button>
        </div>
        <dl>
          <dt>
            <kbd>/</kbd>
          </dt>
          <dd>Focus fleet search</dd>
          <dt>
            <kbd>?</kbd>
          </dt>
          <dd>Open keyboard controls</dd>
          <dt>
            <kbd>Enter</kbd>
          </dt>
          <dd>Open selected topology node controls</dd>
          <dt>
            <kbd>Esc</kbd>
          </dt>
          <dd>Close a dialog, clear selection or search</dd>
        </dl>
      </dialog>
    </div>
  );
}
