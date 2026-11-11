#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Host,
    Provider,
    Session,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NodeKey {
    Host(String),
    Provider(String, String),
    Session(String, String),
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub uri: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub status: String,
    pub host_id: String,
    pub host: String,
    pub provider: String,
    pub advertised: bool,
    pub workspace: String,
    pub agent_count: usize,
    pub session_count: usize,
    pub x: f32,
    pub y: f32,
}

impl Node {
    pub fn key(&self) -> NodeKey {
        match self.kind {
            Kind::Host => NodeKey::Host(self.host_id.to_owned()),
            Kind::Provider => NodeKey::Provider(self.host_id.to_owned(), self.provider.to_owned()),
            Kind::Session => NodeKey::Session(
                self.host_id.to_owned(),
                self.uri.as_ref().expect("session nodes have a URI").clone(),
            ),
        }
    }

    fn matches_key(&self, key: &NodeKey) -> bool {
        match (self.kind, key) {
            (Kind::Host, NodeKey::Host(host)) => self.host_id == *host,
            (Kind::Provider, NodeKey::Provider(host, provider)) => {
                self.host_id == *host && self.provider == *provider
            }
            (Kind::Session, NodeKey::Session(host, uri)) => {
                self.host_id == *host && self.uri.as_deref() == Some(uri.as_str())
            }
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct Edge {
    pub from_x: f32,
    pub from_y: f32,
    pub to_x: f32,
    pub to_y: f32,
    pub bend_y: f32,
    pub stale: bool,
    /// Selection identity, so the light field can prioritise the selected
    /// session's conduit inside its bounded uniform budget.
    pub host_id: String,
    pub session_uri: Option<String>,
}

#[derive(Debug)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub width: f32,
    pub height: f32,
    pub shown: usize,
    pub total: usize,
}

impl Graph {
    pub fn node(&self, key: &NodeKey) -> Option<&Node> {
        self.nodes.iter().find(|node| node.matches_key(key))
    }

    pub fn selected_node(
        &self,
        key: &NodeKey,
        hosts: &[Host<'_>],
        sessions: &[Session<'_>],
    ) -> Option<Node> {
        self.node(key).cloned().or_else(|| match key {
            NodeKey::Session(host, uri) => sessions
                .iter()
                .find(|session| session.host_id == host && session.uri == uri)
                .map(|session| {
                    let online = hosts
                        .iter()
                        .any(|host| host.id == session.host_id && host.status == "connected");
                    session.node(0.0, 0.0, session.host_label(hosts), online)
                }),
            _ => None,
        })
    }
}

pub struct Session<'a> {
    pub host_id: &'a str,
    pub uri: &'a str,
    pub provider: &'a str,
    pub title: &'a str,
    pub status_bits: i64,
    pub activity: &'a str,
    pub working_directories: &'a [&'a str],
    pub prompt: &'a str,
    pub response: &'a str,
    pub tool: Option<(&'a str, &'a str)>,
    pub approval: Option<Approval<'a>>,
}

pub struct Approval<'a> {
    pub title: &'a str,
    pub intention: &'a str,
    pub parameters: &'a str,
    pub approve_label: &'a str,
    pub deny_label: &'a str,
}

pub struct SampleFile {
    pub uri: &'static str,
    pub before: &'static str,
    pub after: &'static str,
}

impl<'a> Session<'a> {
    pub fn status(&self) -> &'static str {
        crate::status::session_state(Some(self.status_bits), Some("ready"), true)
    }

    pub fn host_label<'b>(&'b self, hosts: &'b [Host<'_>]) -> &'b str {
        hosts
            .iter()
            .find(|host| host.id == self.host_id)
            .map_or(self.host_id, |host| host.name)
    }

    pub fn node(&self, x: f32, y: f32, host_label: &str, online: bool) -> Node {
        let status = crate::status::session_state(Some(self.status_bits), Some("ready"), online);
        Node {
            kind: Kind::Session,
            uri: Some(self.uri.to_owned()),
            title: if !self.title.is_empty() {
                self.title
            } else if !self.provider.is_empty() {
                self.provider
            } else {
                "Untitled session"
            }
            .to_owned(),
            subtitle: status.to_owned(),
            status: status.to_owned(),
            host_id: self.host_id.to_owned(),
            host: host_label.to_owned(),
            provider: self.provider.to_owned(),
            advertised: false,
            workspace: self
                .working_directories
                .first()
                .copied()
                .unwrap_or("")
                .to_owned(),
            agent_count: 0,
            session_count: 0,
            x,
            y,
        }
    }

    pub fn workspace_uri(&self) -> &'a str {
        self.working_directories
            .first()
            .copied()
            .filter(|uri| !uri.is_empty())
            .unwrap_or("—")
    }

    pub fn sample_file(&self) -> Option<SampleFile> {
        (self.host_id == "primary" && self.uri == "preview://primary/review").then_some(SampleFile {
            uri: "file:///repo/ai-relay/src/fleet.mjs",
            before: "export function safeError(value) {\n  return String(value || \"Unknown error\");\n}\n",
            after: "export function safeError(value) {\n  return String(value || \"Unknown error\")\n    .replace(/([?&]tkn=)[^&#\\s]+/gi, \"$1[redacted]\");\n}\n",
        })
    }
}

pub struct Host<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub status: &'a str,
    pub providers: &'a [(&'a str, &'a str)],
}

const HOSTS: &[Host<'static>] = &[
    Host {
        id: "primary",
        name: "Primary",
        status: "connected",
        providers: &[
            ("codex", "Codex"),
            ("claude", "Claude"),
            ("opencode", "OpenCode"),
        ],
    },
    Host {
        id: "secondary",
        name: "Secondary",
        status: "connected",
        providers: &[("hermes", "Hermes")],
    },
];

const SESSIONS: &[Session<'static>] = &[
    Session {
        host_id: "primary",
        provider: "codex",
        title: "Review auth",
        uri: "preview://primary/review",
        status_bits: 8,
        activity: "Reading src/fleet.mjs",
        working_directories: &["file:///repo/ai-relay"],
        prompt: "Check the fleet client for token leaks before the next pass.",
        response: "The WebSocket URL still carries the access token as a query parameter. Diagnostics should redact it.",
        tool: Some(("read_file", "completed")),
        approval: None,
    },
    Session {
        host_id: "primary",
        provider: "codex",
        title: "Redact tokens",
        uri: "preview://primary/redact",
        status_bits: 24,
        activity: "Waiting for approval",
        working_directories: &["file:///repo/ai-relay"],
        prompt: "Redact tkn values in handshake errors.",
        response: "I can patch the error formatter. Approval is required before writing the file.",
        tool: None,
        approval: Some(Approval {
            title: "Apply patch",
            intention: "Redact access tokens before they reach diagnostics.",
            parameters: "{\n  \"path\": \"src/fleet.mjs\",\n  \"change\": \"redact tkn query values\"\n}",
            approve_label: "Approve once",
            deny_label: "Deny",
        }),
    },
    Session {
        host_id: "primary",
        provider: "claude",
        title: "Catalog page",
        uri: "preview://primary/catalog",
        status_bits: 1,
        activity: "Idle",
        working_directories: &["file:///repo/ai-relay"],
        prompt: "How many sessions can the topology show at once?",
        response: "The map keeps exceptional sessions first, then caps the visible set. The catalog below still lists the rest.",
        tool: None,
        approval: None,
    },
    Session {
        host_id: "primary",
        provider: "opencode",
        title: "Compare turns",
        uri: "preview://primary/compare",
        status_bits: 1,
        activity: "Idle",
        working_directories: &["file:///repo/ai-relay"],
        prompt: "Leave a sample turn we can scroll back to.",
        response: "This is a local preview turn. It was not produced by an agent host.",
        tool: None,
        approval: None,
    },
    Session {
        host_id: "secondary",
        provider: "hermes",
        title: "Index files",
        uri: "preview://secondary/index",
        status_bits: 1,
        activity: "Idle",
        working_directories: &["file:///work/notes"],
        prompt: "List the notes workspace.",
        response: "Preview only. Secondary has no live filesystem.",
        tool: None,
        approval: None,
    },
    Session {
        host_id: "secondary",
        provider: "hermes",
        title: "Restart check",
        uri: "preview://secondary/restart",
        status_bits: 2,
        activity: "Host reported an error",
        working_directories: &["file:///work/notes"],
        prompt: "What happens if the daemon restarts?",
        response: "Some providers come back under a new session URI. This sample is marked error so that state stays visible.",
        tool: Some(("status", "failed")),
        approval: None,
    },
];

pub fn hosts() -> impl Iterator<Item = (&'static str, &'static str)> {
    HOSTS.iter().map(|host| (host.id, host.name))
}

pub fn host_records() -> &'static [Host<'static>] {
    HOSTS
}

pub fn session_records() -> &'static [Session<'static>] {
    SESSIONS
}

pub fn advertised_agents(host_id: &str) -> impl Iterator<Item = (&'static str, &'static str)> {
    HOSTS
        .iter()
        .find(|host| host.id == host_id)
        .into_iter()
        .flat_map(|host| host.providers.iter().copied())
}

pub fn sessions() -> impl Iterator<Item = &'static Session<'static>> {
    SESSIONS.iter()
}

pub fn catalog_sessions(query: &str) -> impl Iterator<Item = (usize, &'static Session<'static>)> {
    catalog_sessions_for(query, HOSTS, SESSIONS)
}

pub fn catalog_sessions_for<'a>(
    query: &str,
    hosts: &'a [Host<'a>],
    sessions: &'a [Session<'a>],
) -> impl Iterator<Item = (usize, &'a Session<'a>)> {
    let terms = query_terms(query);
    sessions.iter().enumerate().filter(move |(_, session)| {
        let haystack = format!(
            "{} {} {} {}",
            session.host_label(hosts),
            session.title,
            session.provider,
            session.working_directories.join(" ")
        )
        .to_lowercase();
        terms.iter().all(|term| haystack.contains(term))
    })
}

pub fn query_terms(query: &str) -> Vec<String> {
    // JavaScript's \s includes the BOM but excludes the Unicode next-line character.
    query
        .split(|ch: char| (ch.is_whitespace() && ch != '\u{0085}') || ch == '\u{feff}')
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub fn graph() -> Graph {
    graph_for_query("")
}

pub fn graph_for_query(query: &str) -> Graph {
    graph_for_sessions(query, &SESSIONS.iter().collect::<Vec<_>>())
}

pub fn graph_for_sessions(query: &str, sessions: &[&Session<'_>]) -> Graph {
    graph_for_hosts(query, HOSTS, sessions)
}

pub fn graph_for_hosts(query: &str, hosts: &[Host<'_>], sessions: &[&Session<'_>]) -> Graph {
    let terms = query_terms(query);
    let matches = |fields: &[&str]| {
        terms.iter().all(|term| {
            fields
                .iter()
                .any(|field| field.to_lowercase().contains(term))
        })
    };
    let visible: Vec<_> = sessions
        .iter()
        .copied()
        .filter(|session| {
            let host = hosts.iter().find(|host| host.id == session.host_id);
            let agent = host.and_then(|host| {
                host.providers
                    .iter()
                    .find(|(id, _)| *id == session.provider)
            });
            let fields = [
                session.host_id,
                host.map_or("—", |host| host.name),
                session.uri,
                session.title,
                session.provider,
                agent.map_or("", |(_, name)| *name),
                session.activity,
            ];
            terms.iter().all(|term| {
                fields
                    .iter()
                    .chain(session.working_directories.iter())
                    .any(|field| field.to_lowercase().contains(term))
            })
        })
        .collect();
    let mut shown = visible.clone();
    shown.sort_by_key(|session| {
        !matches!(
            session.status(),
            "attention" | "working" | "error" | "connecting" | "reconnecting"
        )
    });
    shown.truncate(16);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut lane_x = 16.0;
    let mut height: f32 = 94.0;
    for host in hosts {
        let host_matches = !terms.is_empty() && matches(&[host.id, host.name, host.status]);
        let host_sessions: Vec<_> = visible
            .iter()
            .copied()
            .filter(|s| s.host_id == host.id)
            .collect();
        let shown_sessions: Vec<_> = shown
            .iter()
            .copied()
            .filter(|s| s.host_id == host.id)
            .collect();
        let mut providers: Vec<_> = host
            .providers
            .iter()
            .filter(|(provider, name)| {
                terms.is_empty()
                    || host_matches
                    || host_sessions.iter().any(|s| s.provider == *provider)
                    || matches(&[provider, name])
            })
            .map(|&(provider, name)| (provider, name, true))
            .collect();
        if !terms.is_empty() && !host_matches && host_sessions.is_empty() && providers.is_empty() {
            continue;
        }
        for session in &shown_sessions {
            if !providers
                .iter()
                .any(|(provider, _, _)| *provider == session.provider)
            {
                let name = if session.provider.is_empty() {
                    "Unknown agent"
                } else {
                    session.provider
                };
                providers.push((session.provider, name, false));
            }
        }
        let columns = providers.len().clamp(1, 2);
        let column_width = (288.0 - (columns - 1) as f32 * 16.0) / columns as f32;
        let mut row_heights = vec![0.0_f32; providers.len().div_ceil(columns)];
        for (index, (provider, _, _)) in providers.iter().enumerate() {
            let count = shown_sessions
                .iter()
                .filter(|s| s.host_id == host.id && s.provider == *provider)
                .count();
            let row = index / columns;
            row_heights[row] = row_heights[row].max(50.0 + count.max(1) as f32 * 40.0);
        }
        let center = lane_x + 160.0;
        nodes.push(Node {
            kind: Kind::Host,
            uri: None,
            title: host.name.to_owned(),
            subtitle: host.status.to_owned(),
            status: host.status.to_owned(),
            host_id: host.id.to_owned(),
            host: host.name.to_owned(),
            provider: String::new(),
            advertised: false,
            workspace: String::new(),
            agent_count: host.providers.len(),
            session_count: host_sessions.len(),
            x: center - 64.0,
            y: 24.0,
        });
        let mut row_y = 80.0;
        for (index, (provider, title, advertised)) in providers.into_iter().enumerate() {
            let row = index / columns;
            let column = index % columns;
            if column == 0 && index > 0 {
                row_y += row_heights[row - 1] + 16.0;
            }
            let x = (lane_x
                + 16.0
                + column as f32 * (column_width + 16.0)
                + (column_width - 128.0) / 2.0)
                .round();
            nodes.push(Node {
                kind: Kind::Provider,
                uri: None,
                title: if !title.is_empty() {
                    title
                } else if !provider.is_empty() {
                    provider
                } else {
                    "Unknown agent"
                }
                .to_owned(),
                subtitle: if advertised {
                    if provider.is_empty() {
                        "Unspecified"
                    } else {
                        provider
                    }
                } else {
                    "Session provider · not advertised"
                }
                .to_owned(),
                status: host.status.to_owned(),
                host_id: host.id.to_owned(),
                host: host.name.to_owned(),
                provider: provider.to_owned(),
                advertised,
                workspace: String::new(),
                agent_count: 0,
                session_count: shown_sessions
                    .iter()
                    .filter(|s| s.host_id == host.id && s.provider == provider)
                    .count(),
                x,
                y: row_y,
            });
            edges.push(Edge {
                from_x: center,
                from_y: 56.0,
                to_x: x + 64.0,
                to_y: row_y,
                bend_y: row_y - 18.0,
                stale: host.status != "connected",
                host_id: host.id.to_owned(),
                session_uri: None,
            });
            let sessions: Vec<_> = shown_sessions
                .iter()
                .filter(|s| s.host_id == host.id && s.provider == provider)
                .copied()
                .collect();

            for (i, session) in sessions.iter().enumerate() {
                let y = row_y + 50.0 + i as f32 * 40.0;
                nodes.push(session.node(x, y, host.name, host.status == "connected"));
                edges.push(Edge {
                    from_x: x + 64.0,
                    from_y: row_y + 32.0,
                    to_x: x + 64.0,
                    to_y: y,
                    bend_y: y - 10.0,
                    stale: host.status != "connected",
                    host_id: session.host_id.to_owned(),
                    session_uri: Some(session.uri.to_owned()),
                });
            }
        }
        let bottom = if row_heights.is_empty() {
            112.0
        } else {
            80.0 + row_heights.iter().sum::<f32>()
                + row_heights.len().saturating_sub(1) as f32 * 16.0
        };
        height = height.max(bottom + 28.0);
        lane_x += 344.0;
    }
    Graph {
        nodes,
        edges,
        width: (lane_x - 24.0_f32 + 16.0).max(680.0),
        height: height.max(176.0),
        shown: shown.len(),
        total: visible.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_renders_both_example_host_lanes() {
        let graph = graph_for_query("");
        assert!(
            !graph.nodes.is_empty(),
            "the example fleet always advertises hosts, so an empty query must render nodes"
        );
        for host_id in ["primary", "secondary"] {
            assert!(
                graph.node(&NodeKey::Host(host_id.to_owned())).is_some(),
                "missing host lane for {host_id}"
            );
        }
        assert_eq!(graph.shown, graph.total);
        assert_eq!(graph.total, SESSIONS.len());
        let session_nodes = graph
            .nodes
            .iter()
            .filter(|node| node.kind == Kind::Session)
            .count();
        assert_eq!(session_nodes, SESSIONS.len());
    }

    #[test]
    fn unmatched_query_renders_no_nodes() {
        let graph = graph_for_query("zzz-no-such-host-or-session");
        assert!(
            graph.nodes.is_empty(),
            "only a query that matches nothing may reach the empty-map state"
        );
    }

    #[test]
    fn query_filters_to_matching_host() {
        let graph = graph_for_query("secondary");
        assert!(!graph.nodes.is_empty());
        assert!(graph.node(&NodeKey::Host("secondary".to_owned())).is_some());
        assert!(graph.node(&NodeKey::Host("primary".to_owned())).is_none());
    }
}
