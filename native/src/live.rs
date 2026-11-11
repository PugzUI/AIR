use ahp::{
    ClientError, TransportError,
    hosts::{HostConfig, HostId, HostState, MultiHostClient},
    transport::BoxedTransport,
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use url::{Host, Url};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    pub uri: String,
    pub title: String,
    pub provider: String,
    pub status: u32,
    pub activity: Option<String>,
    pub modified_at: String,
    pub working_directories: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostView {
    pub id: String,
    pub label: String,
    pub status: &'static str,
    pub last_error: Option<String>,
    pub generation: u64,
    pub protocol_version: Option<String>,
    pub agents: Vec<AgentView>,
    pub sessions: Vec<SessionView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentView {
    pub provider: String,
    pub display_name: String,
    pub models: Vec<ModelView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub disabled: bool,
    pub unconfigured: bool,
}

pub struct SessionSnapshot {
    pub title: String,
    pub provider: String,
    pub status: u32,
    pub lifecycle: String,
    pub creation_error: Option<String>,
    pub attention: Vec<&'static str>,
    pub input_requests: Vec<ChatInputRequestView>,
    pub approval: Option<Approval>,
    /// `toolAuthentication` entries: the web `Attention` fallthrough renders
    /// only the kind plus a resolve-in-host note (no in-app action).
    pub auth_requests: Vec<AuthRequestView>,
    pub working_directories: Vec<String>,
    pub chats: usize,
    pub visible_chats: Vec<(String, String)>,
    pub selected_chat: Option<String>,
    pub conversation: Result<Option<Box<ahp::ahp_types::state::ChatState>>, &'static str>,
    pub changes: Result<Option<(String, Box<ahp::ahp_types::state::ChangesetState>)>, &'static str>,
    pub changeset_count: usize,
    pub available_changesets: Vec<(String, String)>,
    pub changeset_templates: Vec<(String, String)>,
    pub changeset_turns: Vec<(String, String)>,
    pub selected_changeset: Option<String>,
    pub changeset_reviewable: bool,
    pub preview: Option<(String, Result<FilePreview, &'static str>)>,
}

pub struct ChatInputRequestView {
    pub id: String,
    pub chat: String,
    pub request: ahp::ahp_types::state::ChatInputRequest,
}

pub struct AuthRequestView {
    pub kind: &'static str,
}

pub struct Approval {
    pub title: String,
    pub intention: String,
    pub parameters: String,
    /// Buttons to render, in order. Mirrors the frontend: advertised
    /// approve/deny options render as their own buttons; otherwise the two
    /// fallback actions render (possibly disabled).
    pub actions: Vec<ApprovalAction>,
    /// True when the host advertised only non-approve/deny options, in which
    /// case the frontend renders the fallback buttons disabled.
    pub actions_disabled: bool,
    pub chat: String,
    pub turn_id: String,
    pub tool_call_id: String,
    pub result_confirmation: bool,
}

#[derive(Clone)]
pub struct ApprovalAction {
    pub label: String,
    pub approved: bool,
    pub option_id: Option<String>,
}

#[derive(Clone)]
pub struct ToolConfirmationRequest {
    pub host: String,
    pub session: String,
    pub generation: u64,
    pub chat: String,
    pub turn_id: String,
    pub tool_call_id: String,
    pub approved: bool,
    pub result_confirmation: bool,
    pub selected_option_id: Option<String>,
}

#[derive(Clone)]
pub struct InputAnswerRequest {
    pub host: String,
    pub session: String,
    pub generation: u64,
    pub entry_id: String,
    pub chat: String,
    pub request_id: String,
    pub response: ahp::ahp_types::state::ChatInputResponseKind,
    pub answers: Option<std::collections::HashMap<String, ahp::ahp_types::state::ChatInputAnswer>>,
}

#[derive(Clone)]
pub struct FileReviewRequest {
    pub host: String,
    pub session: String,
    pub generation: u64,
    pub changeset: String,
    pub file: String,
    pub reviewed: bool,
}

#[derive(Clone)]
pub struct SessionConfigView {
    pub schema: ahp::ahp_types::state::SessionConfigSchema,
    pub values: ahp::ahp_types::common::JsonObject,
}

pub struct FilePreview {
    pub before: Option<String>,
    pub after: Option<String>,
}

static NEXT_PENDING_MESSAGE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

fn pending_message_id() -> String {
    format!(
        "relay-native-{}",
        NEXT_PENDING_MESSAGE_ID.fetch_add(1, Ordering::Relaxed)
    )
}

pub fn changeset_parameter_names(template: &str) -> Vec<&'static str> {
    let mut names = std::collections::BTreeSet::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let (_, suffix) = rest.split_at(start);
        let Some(end) = suffix.find('}') else {
            return Vec::new();
        };
        let name = &suffix[1..end];
        if matches!(name, "turnId" | "originalTurnId" | "modifiedTurnId") {
            names.insert(match name {
                "turnId" => "turnId",
                "originalTurnId" => "originalTurnId",
                "modifiedTurnId" => "modifiedTurnId",
                _ => unreachable!(),
            });
        } else {
            return Vec::new();
        }
        rest = &suffix[end + 1..];
    }
    names.into_iter().collect()
}

pub fn expand_changeset_with_values(
    template: &str,
    turns: &[&str],
    values: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in turns {
        if !id.is_empty() && seen.insert(*id) {
            ids.push(*id);
        }
    }
    let mut result = String::new();
    let mut rest = template;
    let mut names = std::collections::BTreeSet::new();
    while let Some(start) = rest.find('{') {
        let (prefix, suffix) = rest.split_at(start);
        if prefix.contains('}') {
            return None;
        }
        result.push_str(prefix);
        let end = suffix.find('}')?;
        let name = &suffix[1..end];
        let default = match name {
            "turnId" | "modifiedTurnId" => ids.last(),
            "originalTurnId" => ids.len().checked_sub(2).and_then(|index| ids.get(index)),
            _ => return None,
        }?;
        let id = values.get(name).map(String::as_str).or(Some(*default))?;
        if !ids.contains(&id) {
            return None;
        }
        names.insert(name);
        for byte in id.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
                result.push(char::from(byte));
            } else {
                use std::fmt::Write;
                write!(result, "%{byte:02X}").ok()?;
            }
        }
        rest = &suffix[end + 1..];
    }
    if rest.contains('}')
        || !(names.is_empty()
            || names == ["turnId"].into_iter().collect()
            || names == ["modifiedTurnId", "originalTurnId"].into_iter().collect())
    {
        return None;
    }
    result.push_str(rest);
    Some(result)
}

pub fn expand_changeset(template: &str, turns: &[&str]) -> Option<String> {
    expand_changeset_with_values(template, turns, &std::collections::HashMap::new())
}

/// Display text for a terminal host failure. A transport failure means the
/// socket never opened, so point at the URL and token exactly like the web
/// client; anything else surfaces verbatim.
fn host_failure_message(error: &ClientError) -> String {
    if matches!(error, ClientError::Transport(_)) {
        "WebSocket handshake failed. Check the host URL and access token.".to_owned()
    } else {
        error.to_string()
    }
}

pub fn last_activity(value: Option<&str>) -> String {
    use chrono::{LocalResult, TimeZone};

    value
        .and_then(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|date| date.with_timezone(&chrono::Local))
                .or_else(|| {
                    let bytes = value.as_bytes();
                    if bytes.len() != 10
                        || ![0, 1, 2, 3, 5, 6, 8, 9]
                            .iter()
                            .all(|&index| bytes[index].is_ascii_digit())
                        || bytes[4] != b'-'
                        || bytes[7] != b'-'
                    {
                        return None;
                    }
                    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
                    let midnight = date.and_hms_opt(0, 0, 0)?;
                    Some(
                        chrono::Utc
                            .from_utc_datetime(&midnight)
                            .with_timezone(&chrono::Local),
                    )
                })
                .or_else(|| {
                    let bytes = value.as_bytes();
                    if bytes.len() < 19
                        || ![0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
                            .iter()
                            .all(|&index| bytes[index].is_ascii_digit())
                        || bytes[4] != b'-'
                        || bytes[7] != b'-'
                        || bytes[10] != b'T'
                        || bytes[13] != b':'
                        || bytes[16] != b':'
                    {
                        return None;
                    }
                    let date = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                        .ok()?;
                    match chrono::Local.from_local_datetime(&date) {
                        LocalResult::Single(date) => Some(date),
                        LocalResult::Ambiguous(first, second) => Some(first.min(second)),
                        LocalResult::None => None,
                    }
                })
        })
        .filter(|value| value.timestamp_subsec_nanos() < 1_000_000_000)
        .map(|value| value.format("%Y-%m-%d %H:%M:%S %Z").to_string())
        .unwrap_or_else(|| "Not reported".to_owned())
}

pub fn info_workspace<'a>(
    snapshot: Option<&'a SessionSnapshot>,
    summary: Option<&'a SessionView>,
) -> &'a str {
    snapshot
        .and_then(|snapshot| snapshot.working_directories.first())
        .filter(|directory| !directory.is_empty())
        .or_else(|| {
            summary
                .and_then(|summary| summary.working_directories.first())
                .filter(|directory| !directory.is_empty())
        })
        .map_or("—", String::as_str)
}

pub fn catalog_node(host: &HostView, session: &SessionView) -> crate::example::Node {
    let directories: Vec<&str> = session
        .working_directories
        .iter()
        .map(String::as_str)
        .collect();
    let projected = crate::example::Session {
        host_id: &host.id,
        uri: &session.uri,
        provider: &session.provider,
        title: &session.title,
        status_bits: i64::from(session.status),
        activity: session.activity.as_deref().unwrap_or(""),
        working_directories: &directories,
        prompt: "",
        response: "",
        tool: None,
        approval: None,
    };
    let mut node = projected.node(0.0, 0.0, &host.label, host.status == "connected");
    if session.title.is_empty() {
        node.title = "Untitled session".to_owned();
    }
    node
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostAction {
    Remove,
    Reconnect,
}

pub enum Event {
    Hosts(Vec<HostView>),
    SessionSnapshot {
        request: u64,
        host: String,
        uri: String,
        generation: u64,
        result: Result<Box<SessionSnapshot>, &'static str>,
    },
    AddHost {
        error: Option<&'static str>,
    },
    HostAction {
        id: String,
        action: HostAction,
        error: Option<&'static str>,
    },
    TurnCancellation {
        host: String,
        session: String,
        generation: u64,
        chat: String,
        turn_id: String,
        error: Option<&'static str>,
    },
    TurnQueued {
        host: String,
        session: String,
        generation: u64,
        chat: String,
        message_id: String,
        error: Option<&'static str>,
    },
    ToolConfirmation {
        host: String,
        session: String,
        generation: u64,
        chat: String,
        turn_id: String,
        tool_call_id: String,
        error: Option<&'static str>,
    },
    InputAnswered {
        host: String,
        session: String,
        generation: u64,
        request_id: String,
        error: Option<&'static str>,
    },
    FileReviewChanged {
        host: String,
        session: String,
        generation: u64,
        changeset: String,
        file: String,
        reviewed: bool,
        error: Option<&'static str>,
    },
    TurnsLoaded {
        host: String,
        session: String,
        generation: u64,
        chat: String,
        cursor: String,
        error: Option<&'static str>,
    },
    SessionConfigResolved {
        request: u64,
        host: String,
        generation: u64,
        provider: String,
        directory: String,
        result: Result<Box<SessionConfigView>, &'static str>,
    },
    SessionCreated {
        request: u64,
        host: String,
        generation: u64,
        uri: Option<String>,
        error: Option<&'static str>,
    },
    SessionDisposed {
        request: u64,
        host: String,
        session: String,
        generation: u64,
        error: Option<&'static str>,
    },
}

enum Command {
    Add {
        label: String,
        endpoint: Url,
        token: String,
    },
    Remove(String),
    Reconnect(String),
    CancelTurn {
        host: String,
        session: String,
        generation: u64,
        chat: String,
        turn_id: String,
    },
    QueueTurn {
        host: String,
        session: String,
        generation: u64,
        chat: String,
        message_id: String,
        text: String,
        model: Option<String>,
    },
    ConfirmTool(ToolConfirmationRequest),
    AnswerInput(InputAnswerRequest),
    ReviewFile(FileReviewRequest),
    LoadOlderTurns {
        host: String,
        session: String,
        generation: u64,
        chat: String,
        cursor: String,
    },
    ResolveSessionConfig {
        request: u64,
        host: String,
        generation: u64,
        provider: String,
        directory: String,
        config: ahp::ahp_types::common::JsonObject,
    },
    CreateSession {
        request: u64,
        host: String,
        generation: u64,
        provider: String,
        directory: String,
        config: ahp::ahp_types::common::JsonObject,
        trust: bool,
    },
    DisposeSession {
        request: u64,
        host: String,
        session: String,
        generation: u64,
    },
}

struct QueueTurnRequest<'a> {
    host: &'a str,
    session: &'a str,
    generation: u64,
    chat: &'a str,
    message_id: &'a str,
    text: &'a str,
    model: Option<&'a str>,
}

#[derive(Clone)]
pub enum ChatSelection {
    Preferred(Option<String>),
    Explicit(String),
}

#[derive(Clone)]
pub enum ChangesetSelection {
    Preferred(Option<String>),
    Explicit(String),
}

#[derive(Clone)]
enum SnapshotMode {
    Conversation,
    Full,
}

#[derive(Clone)]
struct SnapshotRequest {
    request: u64,
    host: String,
    uri: String,
    generation: u64,
    chat: ChatSelection,
    changeset: ChangesetSelection,
    file: Option<String>,
    mode: SnapshotMode,
}

pub struct LiveFleet {
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    events: mpsc::Receiver<Event>,
    snapshots: tokio::sync::watch::Sender<Option<SnapshotRequest>>,
}

impl LiveFleet {
    pub fn start() -> std::io::Result<Self> {
        let (commands, mut incoming) = tokio::sync::mpsc::unbounded_channel();
        let (outgoing, events) = mpsc::channel();
        let (snapshots, mut pending_snapshot) =
            tokio::sync::watch::channel(None::<SnapshotRequest>);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        std::thread::Builder::new().name("relay-live".into()).spawn(move || {
            runtime.block_on(async move {
                let multi = MultiHostClient::new();
                let mut timer = tokio::time::interval(Duration::from_millis(400));
                let mut previous = None;
                let mut host_order: Vec<String> = Vec::new();
                let snapshot_multi = multi.clone();
                let snapshot_outgoing = outgoing.clone();
                tokio::spawn(async move {
                    while pending_snapshot.changed().await.is_ok() {
                        let request = pending_snapshot.borrow_and_update().clone();
                        if let Some(SnapshotRequest { request, host, uri, generation, chat, changeset, file, mode }) = request {
                            let result = match session_snapshot(&snapshot_multi, &host, &uri, generation, (&chat, &changeset), mode, &pending_snapshot).await {
                                Ok(snapshot) if file.is_some() && snapshot.changes.is_err() => Err("Changeset is unavailable."),
                                Ok(mut snapshot) => {
                                    if let Some(file) = file {
                                        let result = read_file_preview(&snapshot_multi, &host, &uri, generation, &snapshot, &file, &pending_snapshot).await;
                                        snapshot.preview = Some((file, result));
                                    }
                                    Ok(Box::new(snapshot))
                                }
                                Err(error) => Err(error),
                            };
                            if snapshot_is_obsolete(&pending_snapshot) {
                                continue;
                            }
                            if snapshot_outgoing.send(Event::SessionSnapshot { request, host, uri, generation, result }).is_err() {
                                break;
                            }
                        }
                    }
                });
                loop {
                    tokio::select! {
                        command = incoming.recv() => match command {
                            Some(Command::Add { label, mut endpoint, token }) => {
                                let id = host_id(&label);
                                let host_id = HostId::from(id.clone());
                                if let Some(host) = multi.host(&host_id).await {
                                    if matches!(host.state, HostState::Connected) {
                                        let _ = outgoing.send(Event::AddHost { error: Some("Host is already connected. Select it, or remove it to change its endpoint.") });
                                        continue;
                                    }
                                    if multi.remove_host(&host_id).await.is_err() {
                                        let _ = outgoing.send(Event::AddHost { error: Some("Could not replace the host connection.") });
                                        continue;
                                    }
                                    host_order.retain(|registered| registered != &id);
                                }
                                endpoint.query_pairs_mut().append_pair("tkn", &token);
                                let config = HostConfig::new(host_id, label, move |_| {
                                    let endpoint = endpoint.clone();
                                    async move {
                                        let socket = ahp_ws::WebSocketTransport::connect(endpoint.as_str()).await
                                            .map_err(|_| TransportError::Io("WebSocket connection failed".into()))?;
                                        Ok(BoxedTransport::new(socket))
                                    }
                                });
                                let error = multi.add_host(config).await.err().map(|_| "Could not add the host.");
                                if error.is_none() {
                                    host_order.push(id);
                                }
                                let _ = outgoing.send(Event::AddHost { error });
                                previous = None;
                            }
                            Some(Command::Remove(id)) => {
                                let error = multi.remove_host(&HostId::from(id.clone())).await.err().map(|_| "Could not remove the host.");
                                if error.is_none() {
                                    host_order.retain(|registered| registered != &id);
                                }
                                let _ = outgoing.send(Event::HostAction { id, action: HostAction::Remove, error });
                                previous = None;
                            }
                            Some(Command::Reconnect(id)) => {
                                let error = multi.reconnect_host(&HostId::from(id.clone())).await.err().map(|_| "Could not reconnect the host.");
                                let _ = outgoing.send(Event::HostAction { id, action: HostAction::Reconnect, error });
                                previous = None;
                            }
                            Some(Command::CancelTurn { host, session, generation, chat, turn_id }) => {
                                let error = cancel_turn_request(
                                    &multi,
                                    &host,
                                    &session,
                                    generation,
                                    &chat,
                                    &turn_id,
                                )
                                .await
                                .err();
                                let _ = outgoing.send(Event::TurnCancellation {
                                    host,
                                    session,
                                    generation,
                                    chat,
                                    turn_id,
                                    error,
                                });
                            }
                            Some(Command::QueueTurn { host, session, generation, chat, message_id, text, model }) => {
                                let error = queue_turn_request(
                                    &multi,
                                    QueueTurnRequest {
                                        host: &host,
                                        session: &session,
                                        generation,
                                        chat: &chat,
                                        message_id: &message_id,
                                        text: &text,
                                        model: model.as_deref(),
                                    },
                                )
                                .await
                                .err();
                                let _ = outgoing.send(Event::TurnQueued {
                                    host,
                                    session,
                                    generation,
                                    chat,
                                    message_id,
                                    error,
                                });
                            }
                            Some(Command::ConfirmTool(request)) => {
                                let error = confirm_tool_request(&multi, &request)
                                .await
                                .err();
                                let _ = outgoing.send(Event::ToolConfirmation {
                                    host: request.host,
                                    session: request.session,
                                    generation: request.generation,
                                    chat: request.chat,
                                    turn_id: request.turn_id,
                                    tool_call_id: request.tool_call_id,
                                    error,
                                });
                            }
                            Some(Command::AnswerInput(request)) => {
                                let error = answer_input_request(&multi, &request).await.err();
                                let _ = outgoing.send(Event::InputAnswered {
                                    host: request.host,
                                    session: request.session,
                                    generation: request.generation,
                                    request_id: request.request_id,
                                    error,
                                });
                            }
                            Some(Command::ReviewFile(request)) => {
                                let error = review_file_request(&multi, &request).await.err();
                                let _ = outgoing.send(Event::FileReviewChanged {
                                    host: request.host.clone(),
                                    session: request.session.clone(),
                                    generation: request.generation,
                                    changeset: request.changeset.clone(),
                                    file: request.file.clone(),
                                    reviewed: request.reviewed,
                                    error,
                                });
                            }
                            Some(Command::LoadOlderTurns { host, session, generation, chat, cursor }) => {
                                let error = load_older_turns_request(
                                    &multi,
                                    &host,
                                    &session,
                                    generation,
                                    &chat,
                                    &cursor,
                                )
                                .await
                                .err();
                                let _ = outgoing.send(Event::TurnsLoaded {
                                    host,
                                    session,
                                    generation,
                                    chat,
                                    cursor,
                                    error,
                                });
                            }
                            Some(Command::ResolveSessionConfig { request, host, generation, provider, directory, config }) => {
                                let result = resolve_session_config(
                                    &multi,
                                    &host,
                                    generation,
                                    &provider,
                                    &directory,
                                    config,
                                )
                                .await
                                .map(Box::new);
                                let _ = outgoing.send(Event::SessionConfigResolved {
                                    request,
                                    host,
                                    generation,
                                    provider,
                                    directory,
                                    result,
                                });
                            }
                            Some(Command::CreateSession { request, host, generation, provider, directory, config, trust }) => {
                                let uri = create_session(
                                    &multi,
                                    &host,
                                    generation,
                                    &provider,
                                    &directory,
                                    config,
                                    trust,
                                )
                                .await;
                                let (uri, error) = match uri {
                                    Ok(uri) => (Some(uri), None),
                                    Err(error) => (None, Some(error)),
                                };
                                let _ = outgoing.send(Event::SessionCreated {
                                    request,
                                    host,
                                    generation,
                                    uri,
                                    error,
                                });
                            }
                            Some(Command::DisposeSession {
                                request,
                                host,
                                session,
                                generation,
                            }) => {
                                let error = dispose_session_request(&multi, &host, &session, generation)
                                    .await
                                    .err();
                                let _ = outgoing.send(Event::SessionDisposed {
                                    request,
                                    host,
                                    session,
                                    generation,
                                    error,
                                });
                            }
                            None => break,
                        },
                        _ = timer.tick() => {
                            let mut hosts: Vec<_> = multi.hosts().await.into_iter().map(|host| HostView {
                                id: host.id.to_string(), label: host.label, generation: host.generation,
                                protocol_version: host.protocol_version,
                                agents: host.agents.into_iter().map(|agent| AgentView {
                                    provider: agent.provider,
                                    display_name: agent.display_name,
                                    models: agent.models.into_iter().map(|model| ModelView {
                                        id: model.id,
                                        name: model.name,
                                        disabled: matches!(model.policy_state, Some(ahp::ahp_types::state::PolicyState::Disabled)),
                                        unconfigured: matches!(model.policy_state, Some(ahp::ahp_types::state::PolicyState::Unconfigured)),
                                    }).collect(),
                                }).collect(),
                                sessions: host.session_summaries.into_iter().map(|session| SessionView {
                                    uri: session.resource.to_string(), title: session.title, provider: session.provider,
                                    status: session.status, activity: session.activity, modified_at: session.modified_at,
                                    working_directories: session.working_directories.unwrap_or_default().into_iter().map(|uri| uri.to_string()).collect(),
                                }).collect(),
                                status: match host.state {
                                    HostState::Disconnected => "disconnected",
                                    HostState::Connecting => "connecting",
                                    HostState::Connected => "connected",
                                    HostState::Reconnecting { .. } => "reconnecting",
                                    HostState::Failed { .. } => "failed",
                                    _ => "unknown",
                                },
                                // The supervisor records the failure that drove the
                                // host out of `connected`, so a host that is merely
                                // retrying still explains itself.
                                last_error: host
                                    .last_error
                                    .as_deref()
                                    .map(host_failure_message)
                                    .or_else(|| match &host.state {
                                        HostState::Failed { error } => {
                                            Some(host_failure_message(error))
                                        }
                                        _ => None,
                                    }),
                            }).collect();
                            hosts.sort_by_key(|host| host_order.iter().position(|id| id == &host.id).unwrap_or(usize::MAX));
                            if previous.as_ref() != Some(&hosts) {
                                previous = Some(hosts.clone());
                                if outgoing.send(Event::Hosts(hosts)).is_err() { break; }
                            }
                        }
                    }
                }
                for host in multi.hosts().await {
                    let _ = multi.remove_host(&host.id).await;
                }
            });
        })?;
        Ok(Self {
            commands,
            events,
            snapshots,
        })
    }

    pub fn add(&self, label: &str, endpoint: &str, token: &str) -> Result<(), &'static str> {
        let url = Url::parse(endpoint.trim()).map_err(|_| "Enter a valid WebSocket URL.")?;
        if !matches!(url.scheme(), "ws" | "wss")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.host().is_none()
        {
            return Err("Enter a WebSocket URL without embedded credentials or query parameters");
        }
        let local = matches!(url.host(), Some(Host::Domain("localhost")))
            || matches!(url.host(), Some(Host::Ipv4(ip)) if ip.octets() == [127, 0, 0, 1])
            || matches!(url.host(), Some(Host::Ipv6(ip)) if ip.is_loopback());
        if url.scheme() == "ws" && !local {
            return Err("Remote hosts require encrypted wss://");
        }
        let label = label.trim();
        if host_id(label).is_empty() {
            return Err("Host name is required");
        }
        let token = token.trim();
        if token.is_empty() {
            return Err("An AHP host access token is required");
        }
        self.commands
            .send(Command::Add {
                label: label.into(),
                endpoint: url,
                token: token.into(),
            })
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn remove(&self, id: String) -> Result<(), &'static str> {
        self.commands
            .send(Command::Remove(id))
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn reconnect(&self, id: String) -> Result<(), &'static str> {
        self.commands
            .send(Command::Reconnect(id))
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn cancel_turn(
        &self,
        host: String,
        session: String,
        generation: u64,
        chat: String,
        turn_id: String,
    ) -> Result<(), &'static str> {
        self.commands
            .send(Command::CancelTurn {
                host,
                session,
                generation,
                chat,
                turn_id,
            })
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn queue_turn(
        &self,
        host: String,
        session: String,
        generation: u64,
        chat: String,
        text: String,
        model: Option<String>,
    ) -> Result<String, &'static str> {
        let message_id = pending_message_id();
        self.commands
            .send(Command::QueueTurn {
                host,
                session,
                generation,
                chat,
                message_id: message_id.clone(),
                text,
                model,
            })
            .map_err(|_| "Host connection worker stopped.")?;
        Ok(message_id)
    }

    pub fn confirm_tool(&self, request: ToolConfirmationRequest) -> Result<(), &'static str> {
        self.commands
            .send(Command::ConfirmTool(request))
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn answer_input(&self, request: InputAnswerRequest) -> Result<(), &'static str> {
        self.commands
            .send(Command::AnswerInput(request))
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn review_file(&self, request: FileReviewRequest) -> Result<(), &'static str> {
        self.commands
            .send(Command::ReviewFile(request))
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn load_older_turns(
        &self,
        host: String,
        session: String,
        generation: u64,
        chat: String,
        cursor: String,
    ) -> Result<(), &'static str> {
        self.commands
            .send(Command::LoadOlderTurns {
                host,
                session,
                generation,
                chat,
                cursor,
            })
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn resolve_session_config(
        &self,
        request: u64,
        host: String,
        generation: u64,
        provider: String,
        directory: String,
        config: ahp::ahp_types::common::JsonObject,
    ) -> Result<(), &'static str> {
        self.commands
            .send(Command::ResolveSessionConfig {
                request,
                host,
                generation,
                provider,
                directory,
                config,
            })
            .map_err(|_| "Host connection worker stopped.")
    }

    // Keep this command-shaped API aligned with the host protocol payload.
    #[allow(clippy::too_many_arguments)]
    pub fn create_session(
        &self,
        request: u64,
        host: String,
        generation: u64,
        provider: String,
        directory: String,
        config: ahp::ahp_types::common::JsonObject,
        trust: bool,
    ) -> Result<(), &'static str> {
        self.commands
            .send(Command::CreateSession {
                request,
                host,
                generation,
                provider,
                directory,
                config,
                trust,
            })
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn dispose_session(
        &self,
        request: u64,
        host: String,
        session: String,
        generation: u64,
    ) -> Result<(), &'static str> {
        self.commands
            .send(Command::DisposeSession {
                request,
                host,
                session,
                generation,
            })
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn snapshot(
        &self,
        request: u64,
        host: String,
        uri: String,
        generation: u64,
        chat: ChatSelection,
        changeset: ChangesetSelection,
    ) -> Result<(), &'static str> {
        self.snapshots
            .send(Some(SnapshotRequest {
                request,
                host,
                uri,
                generation,
                chat,
                changeset,
                file: None,
                mode: SnapshotMode::Full,
            }))
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn conversation_snapshot(
        &self,
        request: u64,
        host: String,
        uri: String,
        generation: u64,
        chat: ChatSelection,
    ) -> Result<(), &'static str> {
        self.snapshots
            .send(Some(SnapshotRequest {
                request,
                host,
                uri,
                generation,
                chat,
                changeset: ChangesetSelection::Preferred(None),
                file: None,
                mode: SnapshotMode::Conversation,
            }))
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn preview(
        &self,
        request: u64,
        host: String,
        uri: String,
        generation: u64,
        chat: ChatSelection,
        selection: (String, String),
    ) -> Result<(), &'static str> {
        self.snapshots
            .send(Some(SnapshotRequest {
                request,
                host,
                uri,
                generation,
                chat,
                changeset: ChangesetSelection::Explicit(selection.0),
                file: Some(selection.1),
                mode: SnapshotMode::Full,
            }))
            .map_err(|_| "Host connection worker stopped.")
    }

    pub fn invalidate_snapshot(&self) {
        self.snapshots.send_replace(None);
    }

    pub fn try_event(&self) -> Result<Event, mpsc::TryRecvError> {
        self.events.try_recv()
    }
}

fn snapshot_is_obsolete(pending: &tokio::sync::watch::Receiver<Option<SnapshotRequest>>) -> bool {
    pending.has_changed().unwrap_or(true)
}

fn ensure_snapshot_current(
    pending: &tokio::sync::watch::Receiver<Option<SnapshotRequest>>,
) -> Result<(), &'static str> {
    if snapshot_is_obsolete(pending) {
        Err("Snapshot request superseded.")
    } else {
        Ok(())
    }
}

fn workspace_uri(input: &str) -> Result<String, &'static str> {
    let value = input.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let url = Url::parse(value).map_err(|_| "Workspace must be an absolute file:/// URI")?;
    if url.scheme() != "file"
        || url.host().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().starts_with('/')
    {
        return Err("Workspace must be an absolute file:/// URI on the selected host");
    }
    Ok(url.to_string())
}

fn provider_is_advertised(agents: &[ahp::ahp_types::state::AgentInfo], provider: &str) -> bool {
    agents.iter().any(|agent| agent.provider == provider)
}

async fn resolve_session_config(
    multi: &MultiHostClient,
    host: &str,
    generation: u64,
    provider: &str,
    directory: &str,
    config: ahp::ahp_types::common::JsonObject,
) -> Result<SessionConfigView, &'static str> {
    use ahp::ahp_types::commands::{ResolveSessionConfigParams, ResolveSessionConfigResult};

    let host_id = HostId::from(host.to_owned());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if client.generation() != generation
        || host_view.generation != generation
        || !matches!(host_view.state, HostState::Connected)
    {
        return Err("Host connection changed; reopen the session.");
    }
    if !provider_is_advertised(&host_view.agents, provider) {
        return Err("Provider is not advertised by the selected host");
    }
    let directory = workspace_uri(directory)?;
    let result: ResolveSessionConfigResult = client
        .request(
            "resolveSessionConfig",
            ResolveSessionConfigParams {
                channel: "ahp-root://".to_owned(),
                meta: None,
                provider: Some(provider.to_owned()),
                working_directory: (!directory.is_empty()).then_some(directory),
                config: Some(config),
            },
        )
        .await
        .map_err(|_| "Could not resolve host session options.")?;
    Ok(SessionConfigView {
        schema: result.schema,
        values: result.values,
    })
}

async fn create_session(
    multi: &MultiHostClient,
    host: &str,
    generation: u64,
    provider: &str,
    directory: &str,
    config: ahp::ahp_types::common::JsonObject,
    trust: bool,
) -> Result<String, &'static str> {
    use ahp::ahp_types::{
        actions::{RootConfigChangedAction, StateAction},
        commands::CreateSessionParams,
    };

    let host_id = HostId::from(host.to_owned());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if client.generation() != generation
        || host_view.generation != generation
        || !matches!(host_view.state, HostState::Connected)
    {
        return Err("Host connection changed; reopen the session.");
    }
    if !provider_is_advertised(&host_view.agents, provider) {
        return Err("Provider is not advertised by the selected host");
    }
    let directory = workspace_uri(directory)?;
    if trust && directory.is_empty() {
        return Err("Select a workspace before trusting its project files");
    }
    if trust {
        let mut workspace_trust = serde_json::Map::new();
        workspace_trust.insert("enabled".to_owned(), serde_json::Value::Bool(true));
        workspace_trust.insert(
            "trustedUris".to_owned(),
            serde_json::Value::Array(vec![serde_json::Value::String(directory.clone())]),
        );
        let mut root_config = serde_json::Map::new();
        root_config.insert(
            "workspaceTrust".to_owned(),
            serde_json::Value::Object(workspace_trust),
        );
        client
            .dispatch(
                "ahp-root://".to_owned(),
                StateAction::RootConfigChanged(RootConfigChangedAction {
                    config: root_config,
                    replace: None,
                }),
            )
            .await
            .map_err(|_| "Could not trust the selected workspace.")?;
    }
    let scheme = if provider
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && provider
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    {
        provider
    } else {
        "ahp-session"
    };
    let uri = format!(
        "{scheme}:/relay-native-{}",
        NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed)
    );
    let _: serde_json::Value = client
        .request(
            "createSession",
            CreateSessionParams {
                channel: uri.clone(),
                meta: None,
                provider: Some(provider.to_owned()),
                working_directories: (!directory.is_empty()).then_some(vec![directory]),
                config: (!config.is_empty()).then_some(config),
                active_client: None,
                progress_token: None,
            },
        )
        .await
        .map_err(|_| "Could not create the session on the selected host.")?;
    Ok(uri)
}

async fn dispose_session_request(
    multi: &MultiHostClient,
    host: &str,
    session: &str,
    generation: u64,
) -> Result<(), &'static str> {
    use ahp::ahp_types::commands::DisposeSessionParams;

    let host_id = HostId::from(host.to_owned());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if client.generation() != generation
        || host_view.generation != generation
        || !matches!(host_view.state, HostState::Connected)
    {
        return Err("Host connection changed; reopen the session.");
    }
    if !host_view
        .session_summaries
        .iter()
        .any(|candidate| candidate.resource.as_str() == session)
    {
        return Err("Session is no longer advertised by the selected host.");
    }
    let _: serde_json::Value = client
        .request(
            "disposeSession",
            DisposeSessionParams {
                channel: session.to_owned(),
                meta: None,
            },
        )
        .await
        .map_err(|_| "Could not dispose the session on the selected host.")?;
    Ok(())
}

async fn load_older_turns_request(
    multi: &MultiHostClient,
    host: &str,
    session: &str,
    generation: u64,
    chat: &str,
    cursor: &str,
) -> Result<(), &'static str> {
    use ahp::ahp_types::{
        commands::{FetchTurnsParams, FetchTurnsResult},
        state::SnapshotState,
    };

    if cursor.is_empty() {
        return Err("No older turns available");
    }
    let host_id = HostId::from(host.to_owned());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if client.generation() != generation
        || host_view.generation != generation
        || !matches!(host_view.state, HostState::Connected)
    {
        return Err("Host connection changed; reopen the session.");
    }

    let (session_result, session_subscription) = client
        .raw_client()
        .subscribe(session.to_owned())
        .await
        .map_err(|_| "Could not validate the selected session.")?;
    let session_validation = (|| {
        let snapshot = session_result
            .snapshot
            .ok_or("Host returned no session snapshot.")?;
        if snapshot.resource.as_str() != session {
            return Err("Host returned a snapshot for a different session.");
        }
        let SnapshotState::Session(state) = snapshot.state else {
            return Err("Host returned an unexpected session snapshot.");
        };
        if !state
            .chats
            .iter()
            .any(|candidate| candidate.resource.as_str() == chat)
        {
            return Err("Chat is not part of the selected session");
        }
        Ok(())
    })();
    drop(session_subscription);
    client
        .raw_client()
        .unsubscribe(session.to_owned())
        .await
        .map_err(|_| "Could not release the session validation subscription.")?;
    session_validation?;

    let (chat_result, chat_subscription) = client
        .raw_client()
        .subscribe(chat.to_owned())
        .await
        .map_err(|_| "Could not validate the selected chat.")?;
    let chat_validation = (|| {
        let snapshot = chat_result
            .snapshot
            .ok_or("Host returned no chat snapshot.")?;
        if snapshot.resource.as_str() != chat {
            return Err("Host returned a snapshot for a different chat.");
        }
        let SnapshotState::Chat(state) = snapshot.state else {
            return Err("Host returned an unexpected chat snapshot.");
        };
        if state.turns_next_cursor.as_deref() != Some(cursor) {
            return Err("The turn history changed; refresh the session.");
        }
        Ok(())
    })();
    drop(chat_subscription);
    client
        .raw_client()
        .unsubscribe(chat.to_owned())
        .await
        .map_err(|_| "Could not release the chat validation subscription.")?;
    chat_validation?;

    client
        .check_alive()
        .await
        .map_err(|_| "Host connection changed; reopen the session.")?;
    let _: FetchTurnsResult = client
        .request(
            "fetchTurns",
            FetchTurnsParams {
                channel: chat.to_owned(),
                meta: None,
                cursor: Some(cursor.to_owned()),
            },
        )
        .await
        .map_err(|_| "Could not load earlier turns from the host.")?;
    Ok(())
}

async fn cancel_turn_request(
    multi: &MultiHostClient,
    host: &str,
    session: &str,
    generation: u64,
    chat: &str,
    turn_id: &str,
) -> Result<(), &'static str> {
    use ahp::ahp_types::{
        actions::{ChatTurnCancelledAction, StateAction},
        state::SnapshotState,
    };

    let host_id = HostId::from(host.to_owned());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    if client.generation() != generation {
        return Err("Host connection changed; reopen the session.");
    }
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if host_view.generation != generation || !matches!(host_view.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }

    let (session_result, session_subscription) = client
        .raw_client()
        .subscribe(session.to_owned())
        .await
        .map_err(|_| "Could not validate the selected session.")?;
    let session_validation = (|| {
        let snapshot = session_result
            .snapshot
            .ok_or("Host returned no session snapshot.")?;
        if snapshot.resource.as_str() != session {
            return Err("Host returned a snapshot for a different session.");
        }
        let SnapshotState::Session(state) = snapshot.state else {
            return Err("Host returned an unexpected session snapshot.");
        };
        if !state
            .chats
            .iter()
            .any(|candidate| candidate.resource.as_str() == chat)
        {
            return Err("Chat is not part of the selected session");
        }
        Ok(())
    })();
    drop(session_subscription);
    client
        .raw_client()
        .unsubscribe(session.to_owned())
        .await
        .map_err(|_| "Could not release the session validation subscription.")?;
    session_validation?;

    let (chat_result, chat_subscription) = client
        .raw_client()
        .subscribe(chat.to_owned())
        .await
        .map_err(|_| "Could not validate the selected chat.")?;
    let chat_validation = (|| {
        let snapshot = chat_result
            .snapshot
            .ok_or("Host returned no chat snapshot.")?;
        if snapshot.resource.as_str() != chat {
            return Err("Host returned a snapshot for a different chat.");
        }
        let SnapshotState::Chat(state) = snapshot.state else {
            return Err("Host returned an unexpected chat snapshot.");
        };
        let active = state.active_turn.as_ref().ok_or("No active turn")?;
        if active.id != turn_id {
            return Err("The active turn changed; refresh the session.");
        }
        Ok(())
    })();
    drop(chat_subscription);
    client
        .raw_client()
        .unsubscribe(chat.to_owned())
        .await
        .map_err(|_| "Could not release the chat validation subscription.")?;
    chat_validation?;

    client
        .check_alive()
        .await
        .map_err(|_| "Host connection changed; reopen the session.")?;
    let current = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if current.generation != generation || !matches!(current.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }
    client
        .dispatch(
            chat.to_owned(),
            StateAction::ChatTurnCancelled(ChatTurnCancelledAction {
                turn_id: turn_id.to_owned(),
                duration: 0,
                meta: None,
            }),
        )
        .await
        .map_err(|_| "Could not request turn cancellation.")?;
    Ok(())
}

async fn queue_turn_request(
    multi: &MultiHostClient,
    request: QueueTurnRequest<'_>,
) -> Result<(), &'static str> {
    let QueueTurnRequest {
        host,
        session,
        generation,
        chat,
        message_id,
        text,
        model,
    } = request;
    use ahp::ahp_types::{
        actions::{ChatPendingMessageSetAction, StateAction},
        state::{
            ChatInteractivity, Message, MessageKind, MessageOrigin, ModelSelection,
            PendingMessageKind, PolicyState, SessionLifecycle, SnapshotState,
        },
    };

    let text = text.trim();
    if text.is_empty() {
        return Err("A prompt is required");
    }
    let host_id = HostId::from(host.to_owned());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    if client.generation() != generation {
        return Err("Host connection changed; reopen the session.");
    }
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if host_view.generation != generation || !matches!(host_view.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }

    let (session_result, session_subscription) = client
        .raw_client()
        .subscribe(session.to_owned())
        .await
        .map_err(|_| "Could not validate the selected session.")?;
    let session_validation = (|| {
        let snapshot = session_result
            .snapshot
            .ok_or("Host returned no session snapshot.")?;
        if snapshot.resource.as_str() != session {
            return Err("Host returned a snapshot for a different session.");
        }
        let SnapshotState::Session(state) = snapshot.state else {
            return Err("Host returned an unexpected session snapshot.");
        };
        if matches!(state.lifecycle, SessionLifecycle::Failed) {
            return Err("Session creation failed on the host");
        }
        if matches!(state.lifecycle, SessionLifecycle::Creating) {
            return Err("Session is still starting");
        }
        if !state
            .chats
            .iter()
            .any(|candidate| candidate.resource.as_str() == chat)
        {
            return Err("Chat is not part of the selected session");
        }
        Ok(state.provider.clone())
    })();
    drop(session_subscription);
    client
        .raw_client()
        .unsubscribe(session.to_owned())
        .await
        .map_err(|_| "Could not release the session validation subscription.")?;
    let session_provider = session_validation?;

    let (chat_result, chat_subscription) = client
        .raw_client()
        .subscribe(chat.to_owned())
        .await
        .map_err(|_| "Could not validate the selected chat.")?;
    let validation = (|| {
        let snapshot = chat_result
            .snapshot
            .ok_or("Host returned no chat snapshot.")?;
        if snapshot.resource.as_str() != chat {
            return Err("Host returned a snapshot for a different chat.");
        }
        let SnapshotState::Chat(state) = snapshot.state else {
            return Err("Host returned an unexpected chat snapshot.");
        };
        if matches!(
            state.interactivity,
            Some(ChatInteractivity::ReadOnly | ChatInteractivity::Hidden)
        ) {
            // Mirror the `chatPromptPolicy` queueTurn gate: it throws the
            // policy reason, which interpolates the live interactivity value
            // (the `||` fallback in `queueTurn` never fires — every deny
            // branch carries a reason).
            return Err(match state.interactivity {
                Some(ChatInteractivity::Hidden) => {
                    "This chat is hidden. The host does not accept user prompts here."
                }
                _ => "This chat is read-only. The host does not accept user prompts here.",
            });
        }
        Ok(())
    })();
    drop(chat_subscription);
    client
        .raw_client()
        .unsubscribe(chat.to_owned())
        .await
        .map_err(|_| "Could not release the chat validation subscription.")?;
    validation?;

    client
        .check_alive()
        .await
        .map_err(|_| "Host connection changed; reopen the session.")?;
    let current = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if current.generation != generation || !matches!(current.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }
    let model = model.filter(|model| !model.trim().is_empty());
    if let Some(model_id) = model {
        let agent = current
            .agents
            .iter()
            .find(|agent| agent.provider == session_provider)
            .ok_or("Provider is not advertised by the selected host")?;
        let model_info = agent
            .models
            .iter()
            .find(|candidate| candidate.id == model_id)
            .ok_or("Model is not advertised by this session's provider")?;
        if matches!(model_info.policy_state, Some(PolicyState::Disabled)) {
            return Err("Model is disabled by host policy");
        }
    }
    client
        .dispatch(
            chat.to_owned(),
            StateAction::ChatPendingMessageSet(ChatPendingMessageSetAction {
                kind: PendingMessageKind::Queued,
                id: message_id.to_owned(),
                message: Message {
                    text: text.to_owned(),
                    origin: MessageOrigin {
                        kind: MessageKind::User,
                    },
                    attachments: None,
                    model: model.map(|id| ModelSelection {
                        id: id.to_owned(),
                        config: None,
                    }),
                    agent: None,
                    meta: None,
                },
            }),
        )
        .await
        .map_err(|_| "Could not queue the turn.")?;
    Ok(())
}

async fn confirm_tool_request(
    multi: &MultiHostClient,
    request: &ToolConfirmationRequest,
) -> Result<(), &'static str> {
    use ahp::ahp_types::{
        actions::{ChatToolCallConfirmedAction, ChatToolCallResultConfirmedAction, StateAction},
        state::{
            ConfirmationOptionKind, SessionInputRequest, SnapshotState, ToolCallCancellationReason,
            ToolCallConfirmationReason, ToolCallConfirmationState,
        },
    };
    let host = request.host.clone();
    let session = request.session.clone();
    let generation = request.generation;
    let chat = request.chat.clone();
    let turn_id = request.turn_id.clone();
    let tool_call_id = request.tool_call_id.clone();
    let approved = request.approved;
    let result_confirmation = request.result_confirmation;
    let selected_option_id = request.selected_option_id.clone();
    let host_id = HostId::from(host);
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    if client.generation() != request.generation {
        return Err("Host connection changed; reopen the session.");
    }
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if host_view.generation != request.generation
        || !matches!(host_view.state, HostState::Connected)
    {
        return Err("Host connection changed; reopen the session.");
    }

    let (session_result, session_subscription) = client
        .raw_client()
        .subscribe(request.session.clone())
        .await
        .map_err(|_| "Could not validate the selected session.")?;
    let validation = (|| {
        let snapshot = session_result
            .snapshot
            .ok_or("Host returned no session snapshot.")?;
        if snapshot.resource.as_str() != session {
            return Err("Host returned a snapshot for a different session.");
        }
        let SnapshotState::Session(state) = snapshot.state else {
            return Err("Host returned an unexpected session snapshot.");
        };
        let request = state
            .input_needed
            .unwrap_or_default()
            .into_iter()
            .find_map(|entry| match entry {
                SessionInputRequest::ToolConfirmation(request)
                    if request.chat.as_str() == chat
                        && request.turn_id == turn_id
                        && matches!(
                            &request.tool_call,
                            ToolCallConfirmationState::PendingResultConfirmation(_)
                        ) == result_confirmation =>
                {
                    Some(request)
                }
                _ => None,
            })
            .ok_or("This request is no longer current")?;
        let valid_call = match request.tool_call {
            ToolCallConfirmationState::PendingConfirmation(call) => {
                if call.tool_call_id != tool_call_id {
                    return Err("Tool confirmation changed; review its current state");
                }
                if let Some(option_id) = selected_option_id.as_ref() {
                    let option = call
                        .options
                        .as_ref()
                        .and_then(|options| options.iter().find(|option| option.id == *option_id))
                        .ok_or("Select an approval option from the host")?;
                    if (matches!(option.kind, ConfirmationOptionKind::Approve)) != approved {
                        return Err("Select an approval option matching the requested action.");
                    }
                }
                true
            }
            ToolCallConfirmationState::PendingResultConfirmation(call) => {
                if call.tool_call_id != tool_call_id || selected_option_id.is_some() {
                    return Err("Tool confirmation changed; review its current state");
                }
                true
            }
            ToolCallConfirmationState::Unknown(_) => false,
        };
        if !valid_call {
            return Err("Host returned an unsupported tool confirmation state.");
        }
        Ok(())
    })();
    drop(session_subscription);
    client
        .raw_client()
        .unsubscribe(session.to_owned())
        .await
        .map_err(|_| "Could not release the session validation subscription.")?;
    validation?;

    client
        .check_alive()
        .await
        .map_err(|_| "Host connection changed; reopen the session.")?;
    let current = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if current.generation != generation || !matches!(current.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }

    if result_confirmation {
        client
            .dispatch(
                chat.to_owned(),
                StateAction::ChatToolCallResultConfirmed(ChatToolCallResultConfirmedAction {
                    turn_id: turn_id.to_owned(),
                    tool_call_id: tool_call_id.to_owned(),
                    meta: None,
                    approved,
                }),
            )
            .await
            .map_err(|_| "Could not submit tool result confirmation.")?;
    } else {
        client
            .dispatch(
                chat.to_owned(),
                StateAction::ChatToolCallConfirmed(ChatToolCallConfirmedAction {
                    turn_id: turn_id.to_owned(),
                    tool_call_id: tool_call_id.to_owned(),
                    meta: None,
                    approved,
                    confirmed: approved.then_some(ToolCallConfirmationReason::UserAction),
                    reason: (!approved).then_some(ToolCallCancellationReason::Denied),
                    edited_tool_input: None,
                    user_suggestion: None,
                    reason_message: None,
                    selected_option_id,
                }),
            )
            .await
            .map_err(|_| "Could not submit tool confirmation.")?;
    }
    Ok(())
}

fn approval_from_state(
    state: &ahp::ahp_types::state::SessionState,
    selected_chat: Option<&str>,
) -> Option<Approval> {
    use ahp::ahp_types::state::{
        ConfirmationOptionKind, SessionInputRequest, ToolCallConfirmationState,
    };

    /// First non-empty text wins, mirroring the frontend `a || b || fallback` chains.
    fn nonempty(text: &str) -> Option<&str> {
        (!text.is_empty()).then_some(text)
    }

    state.input_needed.as_ref()?.iter().find_map(|entry| {
        let SessionInputRequest::ToolConfirmation(request) = entry else {
            return None;
        };
        if selected_chat.is_some_and(|chat| request.chat.as_str() != chat) {
            return None;
        }
        let (result_confirmation, title, intention, parameters, tool_call_id, options) =
            match &request.tool_call {
                ToolCallConfirmationState::PendingConfirmation(call) => (
                    false,
                    // Frontend: labelText(confirmationTitle) || displayName || "Tool confirmation".
                    call.confirmation_title
                        .as_ref()
                        .map(|title| title.as_text())
                        .and_then(nonempty)
                        .or_else(|| nonempty(&call.display_name))
                        .unwrap_or("Tool confirmation")
                        .to_owned(),
                    // Frontend: intention || labelText(invocationMessage) || toolName || "Review this tool request".
                    call.intention
                        .as_deref()
                        .and_then(nonempty)
                        .or_else(|| nonempty(call.invocation_message.as_text()))
                        .or_else(|| nonempty(&call.tool_name))
                        .unwrap_or("Review this tool request")
                        .to_owned(),
                    call.tool_input
                        .as_ref()
                        .map_or_else(String::new, |input| format!("{input:?}")),
                    call.tool_call_id.clone(),
                    call.options.as_deref(),
                ),
                ToolCallConfirmationState::PendingResultConfirmation(call) => (
                    true,
                    nonempty(&call.display_name)
                        .unwrap_or("Tool confirmation")
                        .to_owned(),
                    call.intention
                        .as_deref()
                        .and_then(nonempty)
                        .or_else(|| nonempty(call.invocation_message.as_text()))
                        .or_else(|| nonempty(&call.tool_name))
                        .unwrap_or("Review this tool request")
                        .to_owned(),
                    call.tool_input
                        .as_ref()
                        .map_or_else(String::new, |input| format!("{input:?}")),
                    call.tool_call_id.clone(),
                    None,
                ),
                ToolCallConfirmationState::Unknown(_) => return None,
            };
        // Frontend button model: result review always renders the Accept/Reject
        // fallbacks; otherwise advertised approve/deny options render as their
        // own buttons, and the Approve once/Deny fallbacks render only when no
        // approve/deny option was advertised (disabled when the host advertised
        // only other option kinds).
        let (actions, actions_disabled) = if result_confirmation {
            (
                vec![
                    ApprovalAction {
                        label: "Accept result".to_owned(),
                        approved: true,
                        option_id: None,
                    },
                    ApprovalAction {
                        label: "Reject result".to_owned(),
                        approved: false,
                        option_id: None,
                    },
                ],
                false,
            )
        } else {
            let offered: Vec<ApprovalAction> = options
                .iter()
                .flat_map(|options| options.iter())
                .filter(|option| {
                    matches!(
                        option.kind,
                        ConfirmationOptionKind::Approve | ConfirmationOptionKind::Deny
                    )
                })
                .map(|option| ApprovalAction {
                    label: option.label.clone(),
                    approved: matches!(option.kind, ConfirmationOptionKind::Approve),
                    option_id: Some(option.id.clone()),
                })
                .collect();
            if offered.is_empty() {
                (
                    vec![
                        ApprovalAction {
                            label: "Approve once".to_owned(),
                            approved: true,
                            option_id: None,
                        },
                        ApprovalAction {
                            label: "Deny".to_owned(),
                            approved: false,
                            option_id: None,
                        },
                    ],
                    options.is_some_and(|options| !options.is_empty()),
                )
            } else {
                (offered, false)
            }
        };
        Some(Approval {
            title,
            intention,
            parameters,
            actions,
            actions_disabled,
            chat: request.chat.clone(),
            turn_id: request.turn_id.clone(),
            tool_call_id,
            result_confirmation,
        })
    })
}

async fn answer_input_request(
    multi: &MultiHostClient,
    request: &InputAnswerRequest,
) -> Result<(), &'static str> {
    use ahp::ahp_types::{
        actions::{ChatInputCompletedAction, StateAction},
        state::{SessionInputRequest, SnapshotState},
    };

    let host_id = HostId::from(request.host.clone());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    if client.generation() != request.generation {
        return Err("Host connection changed; reopen the session.");
    }
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if host_view.generation != request.generation
        || !matches!(host_view.state, HostState::Connected)
    {
        return Err("Host connection changed; reopen the session.");
    }

    let (session_result, session_subscription) = client
        .raw_client()
        .subscribe(request.session.clone())
        .await
        .map_err(|_| "Could not validate the selected session.")?;
    let validation = (|| {
        let snapshot = session_result
            .snapshot
            .ok_or("Host returned no session snapshot.")?;
        if snapshot.resource.as_str() != request.session {
            return Err("Host returned a snapshot for a different session.");
        }
        let SnapshotState::Session(state) = snapshot.state else {
            return Err("Host returned an unexpected session snapshot.");
        };
        let current = state
            .input_needed
            .unwrap_or_default()
            .into_iter()
            .find_map(|entry| match entry {
                SessionInputRequest::ChatInput(input)
                    if input.id == request.entry_id && input.chat.as_str() == request.chat =>
                {
                    Some(input)
                }
                _ => None,
            });
        let current = current.ok_or("This request is no longer current")?;
        if current.request.id != request.request_id {
            return Err("Input request changed; review its current state");
        }
        Ok(())
    })();
    drop(session_subscription);
    client
        .raw_client()
        .unsubscribe(request.session.clone())
        .await
        .map_err(|_| "Could not release the session validation subscription.")?;
    validation?;

    client
        .check_alive()
        .await
        .map_err(|_| "Host connection changed; reopen the session.")?;
    let current = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if current.generation != request.generation || !matches!(current.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }

    client
        .dispatch(
            request.chat.clone(),
            StateAction::ChatInputCompleted(ChatInputCompletedAction {
                request_id: request.request_id.clone(),
                response: request.response,
                answers: request.answers.clone(),
            }),
        )
        .await
        .map_err(|_| "Could not submit input response.")?;
    Ok(())
}

async fn review_file_request(
    multi: &MultiHostClient,
    request: &FileReviewRequest,
) -> Result<(), &'static str> {
    use ahp::ahp_types::{
        actions::{ChangesetFilesReviewChangedAction, StateAction},
        state::SnapshotState,
    };

    let host_id = HostId::from(request.host.clone());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    if client.generation() != request.generation {
        return Err("Host connection changed; reopen the session.");
    }
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if host_view.generation != request.generation
        || !matches!(host_view.state, HostState::Connected)
    {
        return Err("Host connection changed; reopen the session.");
    }

    let (session_result, session_subscription) = client
        .raw_client()
        .subscribe(request.session.clone())
        .await
        .map_err(|_| "Could not validate the selected session.")?;
    let session_validation = (|| {
        let snapshot = session_result
            .snapshot
            .ok_or("Host returned no session snapshot.")?;
        if snapshot.resource.as_str() != request.session {
            return Err("Host returned a snapshot for a different session.");
        }
        if !matches!(snapshot.state, SnapshotState::Session(_)) {
            return Err("Host returned an unexpected session snapshot.");
        }
        Ok(())
    })();
    drop(session_subscription);
    client
        .raw_client()
        .unsubscribe(request.session.clone())
        .await
        .map_err(|_| "Could not release the session validation subscription.")?;
    session_validation?;

    let (result, subscription) = client
        .raw_client()
        .subscribe(request.changeset.clone())
        .await
        .map_err(|_| "Could not validate the selected changeset.")?;
    let validation = (|| {
        let snapshot = result
            .snapshot
            .ok_or("Host returned no changeset snapshot.")?;
        if snapshot.resource.as_str() != request.changeset {
            return Err("Host returned a snapshot for a different changeset.");
        }
        let SnapshotState::Changeset(state) = snapshot.state else {
            return Err("Host returned an unexpected changeset snapshot.");
        };
        if !state.files.iter().any(|file| file.id == request.file) {
            return Err("Host does not allow reviewing this file");
        }
        Ok(())
    })();
    drop(subscription);
    client
        .raw_client()
        .unsubscribe(request.changeset.clone())
        .await
        .map_err(|_| "Could not release the changeset validation subscription.")?;
    validation?;

    client
        .check_alive()
        .await
        .map_err(|_| "Host connection changed; reopen the session.")?;
    let current = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if current.generation != request.generation || !matches!(current.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }

    client
        .dispatch(
            request.changeset.clone(),
            StateAction::ChangesetFilesReviewChanged(ChangesetFilesReviewChangedAction {
                files: vec![request.file.clone()],
                reviewed: request.reviewed,
            }),
        )
        .await
        .map_err(|_| "Could not update file review state.")?;
    Ok(())
}

async fn session_snapshot(
    multi: &MultiHostClient,
    host: &str,
    uri: &str,
    generation: u64,
    selection: (&ChatSelection, &ChangesetSelection),
    mode: SnapshotMode,
    pending: &tokio::sync::watch::Receiver<Option<SnapshotRequest>>,
) -> Result<SessionSnapshot, &'static str> {
    use ahp::ahp_types::state::{
        ChatInteractivity, SessionInputRequest, SessionLifecycle, SnapshotState,
    };

    let (chat_selection, changeset_selection) = selection;

    ensure_snapshot_current(pending)?;
    let host_id = HostId::from(host.to_owned());
    let client = multi
        .client(&host_id)
        .await
        .ok_or("Host is not connected")?;
    if client.generation() != generation {
        return Err("Host connection changed; reopen the session.");
    }
    let (result, subscription) = client
        .raw_client()
        .subscribe(uri.to_owned())
        .await
        .map_err(|_| "Could not load the session snapshot.")?;
    drop(subscription);
    client
        .raw_client()
        .unsubscribe(uri.to_owned())
        .await
        .map_err(|_| "Could not release the session subscription.")?;
    ensure_snapshot_current(pending)?;
    client
        .check_alive()
        .await
        .map_err(|_| "Host connection changed; reopen the session.")?;
    let host_view = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if host_view.generation != generation || !matches!(host_view.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }
    let snapshot = result
        .snapshot
        .ok_or("Host returned no session snapshot.")?;
    if snapshot.resource.as_str() != uri {
        return Err("Host returned a snapshot for a different resource.");
    }
    let SnapshotState::Session(state) = snapshot.state else {
        return Err("Host returned an unexpected snapshot type.");
    };
    let available: Vec<_> = state
        .chats
        .iter()
        .filter(|chat| chat.interactivity != Some(ChatInteractivity::Hidden))
        .collect();
    let find_chat = |uri: &str| {
        available
            .iter()
            .find(|chat| chat.resource.as_str() == uri)
            .copied()
    };
    let selected = match chat_selection {
        ChatSelection::Explicit(uri) => {
            Some(find_chat(uri).ok_or("Chat is not part of the selected session")?)
        }
        ChatSelection::Preferred(uri) => uri
            .as_deref()
            .and_then(find_chat)
            .or_else(|| {
                state
                    .default_chat
                    .as_ref()
                    .and_then(|uri| find_chat(uri.as_str()))
            })
            .or_else(|| available.first().copied()),
    };
    let visible_chats = available
        .iter()
        .map(|chat| (chat.resource.to_string(), chat.title.clone()))
        .collect();
    let selected_chat = selected.map(|chat| chat.resource.to_string());
    let conversation = if let Some(chat) = selected {
        let chat_uri = chat.resource.to_string();
        match client.raw_client().subscribe(chat_uri.clone()).await {
            Ok((result, subscription)) => {
                let outcome = (|| {
                    let snapshot = result.snapshot.ok_or("Host returned no chat snapshot.")?;
                    if snapshot.resource.as_str() != chat_uri {
                        return Err("Host returned a snapshot for a different chat.");
                    }
                    u64::try_from(snapshot.from_seq)
                        .map_err(|_| "Host returned an invalid chat sequence.")?;
                    let SnapshotState::Chat(chat) = snapshot.state else {
                        return Err("Host returned an unexpected chat snapshot.");
                    };
                    if chat.resource.as_str() != chat_uri {
                        return Err("Host returned an unexpected chat snapshot.");
                    }
                    Ok(Some(chat))
                })();
                drop(subscription);
                match client.raw_client().unsubscribe(chat_uri).await {
                    Err(_) => Err("Could not release the chat subscription."),
                    Ok(()) => outcome,
                }
            }
            Err(_) => Err("Could not load the chat snapshot."),
        }
    } else {
        Ok(None)
    };
    ensure_snapshot_current(pending)?;
    let session_changes = state.changesets.as_deref().unwrap_or_default();
    let chat_changes = conversation
        .as_ref()
        .ok()
        .and_then(|chat| chat.as_ref())
        .and_then(|chat| chat.changesets.as_deref())
        .unwrap_or_default();
    let entries: Vec<_> = session_changes.iter().chain(chat_changes).collect();
    let changeset_count = entries.len();
    let turns: Vec<_> = conversation
        .as_ref()
        .ok()
        .and_then(|chat| chat.as_ref())
        .map(|chat| chat.turns.iter().map(|turn| turn.id.as_str()).collect())
        .unwrap_or_default();
    let changeset_turns: Vec<_> = conversation
        .as_ref()
        .ok()
        .and_then(|chat| chat.as_ref())
        .map(|chat| {
            chat.turns
                .iter()
                .map(|turn| (turn.id.clone(), turn.message.text.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let available_entries: Vec<_> = entries
        .into_iter()
        .filter_map(|entry| expand_changeset(&entry.uri_template, &turns).map(|uri| (entry, uri)))
        .filter(|(_, uri)| seen.insert(uri.clone()))
        .collect();
    let mut expanded_seen = std::collections::HashSet::new();
    let mut expanded_entries = Vec::new();
    for entry in session_changes.iter().chain(chat_changes) {
        let parameters = changeset_parameter_names(&entry.uri_template);
        let values = match parameters.as_slice() {
            [] => vec![std::collections::HashMap::new()],
            ["turnId"] | ["modifiedTurnId"] => turns
                .iter()
                .map(|turn| {
                    std::collections::HashMap::from([(
                        parameters[0].to_owned(),
                        (*turn).to_owned(),
                    )])
                })
                .collect(),
            ["modifiedTurnId", "originalTurnId"] | ["originalTurnId", "modifiedTurnId"] => {
                let mut values = Vec::new();
                for original in &turns {
                    for modified in &turns {
                        values.push(std::collections::HashMap::from([
                            ("originalTurnId".to_owned(), (*original).to_owned()),
                            ("modifiedTurnId".to_owned(), (*modified).to_owned()),
                        ]));
                    }
                }
                values
            }
            _ => Vec::new(),
        };
        for values in values {
            if let Some(uri) = expand_changeset_with_values(&entry.uri_template, &turns, &values)
                && expanded_seen.insert(uri.clone())
            {
                expanded_entries.push((entry, uri));
            }
        }
    }
    let preferred = available_entries
        .iter()
        .find(|(entry, _)| entry.change_kind == "session")
        .or_else(|| {
            available_entries
                .iter()
                .find(|(entry, _)| entry.change_kind == "uncommitted")
        })
        .or_else(|| available_entries.first());
    let chosen = match changeset_selection {
        ChangesetSelection::Explicit(uri) => Some(
            expanded_entries
                .iter()
                .find(|(_, candidate)| candidate == uri)
                .ok_or("Changeset is not advertised by the selected session")?,
        ),
        ChangesetSelection::Preferred(uri) => uri
            .as_ref()
            .and_then(|uri| {
                expanded_entries
                    .iter()
                    .find(|(_, candidate)| candidate == uri)
            })
            .or(preferred),
    };
    let available_changesets = available_entries
        .iter()
        .map(|(entry, uri)| (uri.clone(), entry.label.clone()))
        .collect();
    let changeset_templates = available_entries
        .iter()
        .map(|(entry, uri)| (uri.clone(), entry.uri_template.clone()))
        .collect();
    let selected_changeset = chosen.map(|(_, uri)| uri.clone());
    let changeset_reviewable = chosen.is_some_and(|(entry, _)| {
        entry
            .capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.review.is_some())
    });
    let changes = if matches!(mode, SnapshotMode::Conversation) {
        Err("Files were not loaded; refresh the Files tab.")
    } else if let Some((entry, uri)) = chosen {
        let uri = uri.clone();
        match client.raw_client().subscribe(uri.clone()).await {
            Ok((result, subscription)) => {
                drop(subscription);
                match client.raw_client().unsubscribe(uri.clone()).await {
                    Err(_) => Err("Could not release the changeset subscription."),
                    Ok(()) => match result.snapshot {
                        Some(snapshot) if snapshot.resource.as_str() == uri => match snapshot.state
                        {
                            SnapshotState::Changeset(changes) => {
                                Ok(Some((entry.label.clone(), changes)))
                            }
                            _ => Err("Host returned an unexpected changeset snapshot."),
                        },
                        Some(_) => Err("Host returned a snapshot for a different changeset."),
                        None => Err("Host returned no changeset snapshot."),
                    },
                }
            }
            Err(_) => Err("Could not load the changeset snapshot."),
        }
    } else if conversation.is_err() {
        Err("Chat changeset catalog is unavailable.")
    } else {
        Ok(None)
    };
    ensure_snapshot_current(pending)?;
    client
        .check_alive()
        .await
        .map_err(|_| "Host connection changed; reopen the session.")?;
    let current = multi
        .host(&host_id)
        .await
        .ok_or("Host is no longer available.")?;
    if current.generation != generation || !matches!(current.state, HostState::Connected) {
        return Err("Host connection changed; reopen the session.");
    }
    let input_requests = state
        .input_needed
        .as_ref()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| match entry {
                    SessionInputRequest::ChatInput(input) => Some(ChatInputRequestView {
                        id: input.id.clone(),
                        chat: input.chat.to_string(),
                        request: input.request.clone(),
                    }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let approval = approval_from_state(&state, selected_chat.as_deref());
    let auth_requests = state
        .input_needed
        .as_ref()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| match entry {
                    // Mirror `attentionFor`: only `toolAuthentication` reaches
                    // the `Attention` fallthrough (`toolConfirmation` renders
                    // the approval panel, `chatInput` the answer form, and
                    // `toolClientExecution`/unknown kinds are filtered out).
                    SessionInputRequest::ToolAuthentication(_) => Some(AuthRequestView {
                        kind: "toolAuthentication",
                    }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let lifecycle = match state.lifecycle {
        SessionLifecycle::Creating => "creating".to_owned(),
        SessionLifecycle::Ready => "ready".to_owned(),
        SessionLifecycle::Failed => "failed".to_owned(),
        SessionLifecycle::Unknown(value) => value,
    };
    Ok(SessionSnapshot {
        title: state.title,
        provider: state.provider,
        status: state.status,
        lifecycle,
        creation_error: state.creation_error.map(|error| error.message),
        attention: state
            .input_needed
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| match entry {
                SessionInputRequest::ChatInput(_) => Some("Input requested"),
                SessionInputRequest::ToolConfirmation(_) => Some("Tool confirmation requested"),
                SessionInputRequest::ToolAuthentication(_) => Some("Tool authentication requested"),
                _ => None,
            })
            .collect(),
        input_requests,
        approval,
        auth_requests,
        working_directories: state
            .working_directories
            .unwrap_or_default()
            .into_iter()
            .map(|uri| uri.to_string())
            .collect(),
        chats: state.chats.len(),
        visible_chats,
        selected_chat,
        conversation,
        changes,
        changeset_count,
        available_changesets,
        changeset_templates,
        changeset_turns,
        selected_changeset,
        changeset_reviewable,
        preview: None,
    })
}

async fn read_preview_side(
    client: &ahp::hosts::HostClientHandle,
    side: Option<&ahp::ahp_types::state::FileEditSide>,
) -> Result<Option<String>, &'static str> {
    use ahp::ahp_types::commands::{ContentEncoding, ResourceReadParams, ResourceReadResult};
    let Some(side) = side else { return Ok(None) };
    let content = &side.content;
    if content.uri.is_empty() {
        return Ok(None);
    }
    if content
        .size_hint
        .is_some_and(|size| size > MAX_PREVIEW_BYTES as i64)
    {
        return Err("File preview exceeds 256 KiB; inspect it in an editor");
    }
    let response: ResourceReadResult = tokio::time::timeout(
        Duration::from_secs(15),
        client.request(
            "resourceRead",
            ResourceReadParams {
                channel: "ahp-root://".into(),
                meta: None,
                uri: content.uri.to_string(),
                encoding: Some(ContentEncoding::Utf8),
            },
        ),
    )
    .await
    .map_err(|_| "File content request timed out.")?
    .map_err(|_| "Could not read file content from the host.")?;
    decode_preview_text(response).map(Some)
}

const MAX_PREVIEW_BYTES: usize = 256 * 1024;

pub(crate) fn decode_preview_text(
    response: ahp::ahp_types::commands::ResourceReadResult,
) -> Result<String, &'static str> {
    use ahp::ahp_types::commands::ContentEncoding;
    use base64::{
        Engine, alphabet,
        engine::{
            DecodePaddingMode,
            general_purpose::{GeneralPurpose, GeneralPurposeConfig},
        },
    };
    if let Some(kind) = response
        .content_type
        .as_deref()
        .filter(|kind| !kind.is_empty())
    {
        let text = kind.starts_with("text/")
            || kind.strip_prefix("application/").is_some_and(|value| {
                value.starts_with("json")
                    || value.starts_with("xml")
                    || value.starts_with("javascript")
                    || value.contains("+json")
                    || value.contains("+xml")
            });
        if !text {
            return Err("The selected file is not text; use an editor for binary files");
        }
    }
    let mut bytes = match response.encoding {
        ContentEncoding::Utf8 => response.data.into_bytes(),
        ContentEncoding::Base64 => {
            if response.data.len() > MAX_PREVIEW_BYTES.div_ceil(3) * 4 + 8 {
                return Err("File preview exceeds 256 KiB; inspect it in an editor");
            }
            let data: Vec<u8> = response
                .data
                .bytes()
                .filter(|byte| !matches!(byte, b'\t' | b'\n' | b'\x0c' | b'\r' | b' '))
                .collect();
            if data.contains(&b'=') && !data.len().is_multiple_of(4) {
                return Err("Host returned invalid base64 content");
            }
            let engine = GeneralPurpose::new(
                &alphabet::STANDARD,
                GeneralPurposeConfig::new()
                    .with_decode_padding_mode(DecodePaddingMode::Indifferent)
                    .with_decode_allow_trailing_bits(true),
            );
            engine
                .decode(data)
                .map_err(|_| "Host returned invalid base64 content")?
        }
    };
    // TextDecoder removes a UTF-8 BOM for Base64 content, but not direct strings.
    if response.encoding == ContentEncoding::Base64 && bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        bytes.drain(..3);
    }
    if bytes.len() > MAX_PREVIEW_BYTES {
        return Err("File preview exceeds 256 KiB; inspect it in an editor");
    }
    let text = String::from_utf8(bytes).map_err(|_| "Host returned invalid UTF-8 content")?;
    if text.contains('\0') {
        return Err("The selected file appears to be binary");
    }
    Ok(text)
}

async fn read_file_preview(
    multi: &MultiHostClient,
    host: &str,
    session: &str,
    generation: u64,
    snapshot: &SessionSnapshot,
    file_id: &str,
    pending: &tokio::sync::watch::Receiver<Option<SnapshotRequest>>,
) -> Result<FilePreview, &'static str> {
    ensure_snapshot_current(pending)?;
    let changeset = snapshot
        .selected_changeset
        .as_ref()
        .ok_or("Select a host changeset first")?;
    let files = &snapshot
        .changes
        .as_ref()
        .map_err(|_| "Changeset is unavailable.")?
        .as_ref()
        .ok_or("Select a host changeset first")?
        .1
        .files;
    let file = files
        .iter()
        .find(|file| file.id == file_id)
        .ok_or("File is no longer in the current changeset")?;
    let client = multi
        .client(&HostId::from(host.to_owned()))
        .await
        .ok_or("Host is not connected")?;
    if client.generation() != generation {
        return Err("Host connection changed; reopen the session.");
    }
    let (before, after) = tokio::try_join!(
        read_preview_side(&client, file.edit.before.as_ref()),
        read_preview_side(&client, file.edit.after.as_ref()),
    )?;
    ensure_snapshot_current(pending)?;
    let current = session_snapshot(
        multi,
        host,
        session,
        generation,
        (
            &ChatSelection::Preferred(snapshot.selected_chat.clone()),
            &ChangesetSelection::Explicit(changeset.clone()),
        ),
        SnapshotMode::Full,
        pending,
    )
    .await?;
    let latest = current
        .changes
        .as_ref()
        .map_err(|_| "Changeset is unavailable.")?
        .as_ref()
        .and_then(|(_, changes)| changes.files.iter().find(|item| item.id == file_id));
    let same = |a: Option<&ahp::ahp_types::state::FileEditSide>,
                b: Option<&ahp::ahp_types::state::FileEditSide>| {
        a.map(|side| (&side.content.uri, &side.content.nonce))
            == b.map(|side| (&side.content.uri, &side.content.nonce))
    };
    if current.selected_chat != snapshot.selected_chat
        || !latest.is_some_and(|latest| {
            same(file.edit.before.as_ref(), latest.edit.before.as_ref())
                && same(file.edit.after.as_ref(), latest.edit.after.as_ref())
        })
    {
        return Err("File or changeset changed while reading; reopen the file");
    }
    Ok(FilePreview { before, after })
}

/// Sessions in fleet order: hosts as stored, then each host's sessions as
/// stored. Mirrors `fleet.sessions()` (backed by `aggregatedSessions`), which
/// neither the Sessions catalog nor `buildGraph` re-sorts — the topology's
/// priority sort is stable and preserves this order within each group.
pub fn fleet_sessions(hosts: &[HostView]) -> Vec<(&HostView, &SessionView)> {
    hosts
        .iter()
        .flat_map(|host| host.sessions.iter().map(move |session| (host, session)))
        .collect()
}

/// Working, attention and idle sessions across the fleet, so the legend can
/// print a count for every state it names, zeros included.
pub fn status_counts(hosts: &[HostView]) -> (usize, usize, usize) {
    let mut working = 0;
    let mut attention = 0;
    let mut idle = 0;
    for host in hosts {
        for session in &host.sessions {
            match crate::status::session_state(
                Some(i64::from(session.status)),
                None,
                host.status == "connected",
            ) {
                "working" => working += 1,
                "attention" => attention += 1,
                "idle" => idle += 1,
                _ => {}
            }
        }
    }
    (working, attention, idle)
}

pub fn graph(hosts: &[HostView], query: &str) -> crate::example::Graph {
    let providers: Vec<Vec<(&str, &str)>> = hosts
        .iter()
        .map(|host| {
            host.agents
                .iter()
                .map(|agent| (agent.provider.as_str(), agent.display_name.as_str()))
                .collect()
        })
        .collect();
    let projected_hosts: Vec<_> = hosts
        .iter()
        .zip(&providers)
        .map(|(host, providers)| crate::example::Host {
            id: &host.id,
            name: &host.label,
            status: host.status,
            providers,
        })
        .collect();
    let sessions = fleet_sessions(hosts);
    let directories: Vec<Vec<&str>> = sessions
        .iter()
        .map(|(_, session)| {
            session
                .working_directories
                .iter()
                .map(String::as_str)
                .collect()
        })
        .collect();
    let projected_sessions: Vec<_> = sessions
        .iter()
        .zip(&directories)
        .map(|((host, session), directories)| crate::example::Session {
            host_id: &host.id,
            uri: &session.uri,
            provider: &session.provider,
            title: &session.title,
            status_bits: i64::from(session.status),
            activity: session.activity.as_deref().unwrap_or(""),
            working_directories: directories,
            prompt: "",
            response: "",
            tool: None,
            approval: None,
        })
        .collect();
    crate::example::graph_for_hosts(
        query,
        &projected_hosts,
        &projected_sessions.iter().collect::<Vec<_>>(),
    )
}

fn host_id(label: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for ch in label.trim().to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-' {
            result.push(ch);
            separator = false;
        } else if !separator {
            result.push('-');
            separator = true;
        }
    }
    result
}
