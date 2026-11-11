use relay_native::{conversation, example, lightfield, live, options, spec, status, theme};

use example::{Graph, Kind, Node, NodeKey};
use gpui::{
    AnyElement, App, Application, Bounds, Context, DispatchPhase, Entity, FocusHandle, Focusable,
    KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollHandle, Timer, TitlebarOptions, Window, WindowBounds, WindowOptions, canvas, div,
    point, prelude::*, px, rgb, rgba, size,
};
use gpui::{ImageSource, ObjectFit, RenderImage, fill, img};
use gpui_component::{
    Root, StyledExt,
    input::{Input, InputState},
    scroll::ScrollableElement,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use url::Url;

const DEFAULT_MODEL_ID: &str = "<host-default>";
/// The bounded host-confirmed pulse window, matching the browser shader.
const LIGHTFIELD_PULSE: Duration = Duration::from_millis(1400);
const LIGHTFIELD_PULSE_FRAME: Duration = Duration::from_millis(50);

fn live_model_focus_key(host: &str, provider: &str, model: &str) -> String {
    format!("{host}\0{provider}\0{model}")
}

fn create_agent_focus_key(host: &str, provider: &str) -> String {
    format!("{host}\0{provider}")
}

fn config_input_value(
    value: Option<&serde_json::Value>,
    property: &ahp::ahp_types::state::SessionConfigPropertySchema,
) -> String {
    let kind = property.r#type.as_str();
    let string_items = kind == "array"
        && property
            .items
            .as_ref()
            .is_some_and(|items| items.r#type == "string");
    match value {
        Some(serde_json::Value::String(value)) => value.clone(),
        // Mirror the web form: string arrays edit as one entry per line.
        Some(serde_json::Value::Array(items)) if string_items => items
            .iter()
            .map(|item| match item {
                serde_json::Value::String(item) => item.clone(),
                _ => item.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        // Objects and non-string arrays edit as JSON, pretty-printed like the web form.
        Some(value) if kind == "object" || kind == "array" => {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        }
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn config_enum_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn config_is_static_enum(property: &ahp::ahp_types::state::SessionConfigPropertySchema) -> bool {
    property
        .r#enum
        .as_ref()
        .is_some_and(|choices| !choices.is_empty())
        && property.enum_dynamic != Some(true)
}

fn config_enum_label(
    property: &ahp::ahp_types::state::SessionConfigPropertySchema,
    index: usize,
) -> String {
    let choice = property
        .r#enum
        .as_ref()
        .and_then(|choices| choices.get(index));
    if let Some(label) = property
        .enum_labels
        .as_ref()
        .and_then(|labels| labels.get(index))
    {
        return label.clone();
    }
    choice.map(config_enum_value).unwrap_or_default()
}

fn create_config_option_key(name: &str, suffix: &str) -> String {
    format!("{name}\0{suffix}")
}

struct PendingHostAction {
    id: String,
    action: live::HostAction,
    generation: u64,
    acknowledged: bool,
}

struct PendingTurnCancellation {
    host: String,
    session: String,
    generation: u64,
    chat: String,
    turn_id: String,
}

struct PendingTurnSubmission {
    host: String,
    session: String,
    generation: u64,
    chat: String,
    message_id: String,
}

struct PendingTurnPage {
    host: String,
    session: String,
    generation: u64,
    chat: String,
    cursor: String,
}

struct PendingFileReview {
    host: String,
    session: String,
    generation: u64,
    changeset: String,
    file: String,
    reviewed: bool,
}

struct PendingToolConfirmation {
    host: String,
    session: String,
    generation: u64,
    chat: String,
    turn_id: String,
    tool_call_id: String,
}

struct PendingSessionDisposal {
    request: u64,
    host: String,
    session: String,
    generation: u64,
    acknowledged: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionTab {
    Conversation,
    Files,
    Info,
}

const SESSION_TABS: [(SessionTab, &str); 3] = [
    (SessionTab::Conversation, "Conversation"),
    (SessionTab::Files, "Files"),
    (SessionTab::Info, "Info"),
];

struct Relay {
    hosts_expanded: bool,
    hosts_toggle_focus: FocusHandle,
    host_focus: HashMap<&'static str, FocusHandle>,
    hovered_host: Option<&'static str>,
    active_host: Option<String>,
    sessions_expanded: bool,
    sessions_toggle_focus: FocusHandle,
    catalog_focus: Vec<FocusHandle>,
    search: Entity<InputState>,
    clear_search_focus: FocusHandle,
    live: bool,
    live_fleet: Option<live::LiveFleet>,
    live_hosts: Vec<live::HostView>,
    live_host_focus: HashMap<String, FocusHandle>,
    live_session_focus: HashMap<NodeKey, FocusHandle>,
    live_remove_focus: HashMap<String, FocusHandle>,
    live_reconnect_focus: HashMap<String, FocusHandle>,
    live_model_focus: HashMap<String, FocusHandle>,
    create_agent_focus: HashMap<String, FocusHandle>,
    live_model: Option<String>,
    live_prompt: Entity<InputState>,
    queue_turn_focus: FocusHandle,
    cancel_turn_focus: FocusHandle,
    restart_worker_focus: FocusHandle,
    pending_turn_cancellation: Option<PendingTurnCancellation>,
    pending_turn_submission: Option<PendingTurnSubmission>,
    pending_turn_page: Option<PendingTurnPage>,
    pending_file_review: Option<PendingFileReview>,
    pending_tool_confirmation: Option<PendingToolConfirmation>,
    pending_input_submission: Option<String>,
    pending_session_disposal: Option<PendingSessionDisposal>,
    pending_host_action: Option<PendingHostAction>,
    live_error: Option<String>,
    connect_open: bool,
    connect_pending: bool,
    connect_label: Entity<InputState>,
    connect_endpoint: Entity<InputState>,
    connect_token: Entity<InputState>,
    connect_button_focus: FocusHandle,
    connect_close_focus: FocusHandle,
    connect_submit_focus: FocusHandle,
    create_open: bool,
    create_pending: bool,
    create_request: u64,
    create_provider: Option<String>,
    create_directory: Entity<InputState>,
    create_trust: bool,
    create_trust_focus: FocusHandle,
    create_config: Option<live::SessionConfigView>,
    create_config_directory: Option<String>,
    create_config_inputs: HashMap<String, Entity<InputState>>,
    create_config_enums: HashMap<String, Option<usize>>,
    create_config_bools: HashMap<String, Option<bool>>,
    create_config_option_focus: HashMap<String, FocusHandle>,
    create_refresh_focus: FocusHandle,
    create_submit_focus: FocusHandle,
    dispose_confirm_open: bool,
    dispose_request: u64,
    dispose_focus: FocusHandle,
    dispose_confirm_focus: FocusHandle,
    dispose_cancel_focus: FocusHandle,
    mode_focus: FocusHandle,
    shortcuts_open: bool,
    shortcuts_button_focus: FocusHandle,
    shortcuts_close_focus: FocusHandle,
    shortcuts_return_focus: Option<FocusHandle>,
    status_open: bool,
    status_button_focus: FocusHandle,
    status_close_focus: FocusHandle,
    status_dismiss_focus: FocusHandle,
    banner_dismiss_focus: FocusHandle,
    status_return_focus: Option<FocusHandle>,
    selected: Option<NodeKey>,
    live_preview: Option<Node>,
    snapshot_request: u64,
    session_snapshot: Option<Result<live::SessionSnapshot, &'static str>>,
    pending_file_preview: Option<String>,
    selected_chat: Option<String>,
    chat_focus: HashMap<String, FocusHandle>,
    input_focus: HashMap<String, FocusHandle>,
    input_option_focus: HashMap<String, FocusHandle>,
    input_inputs: HashMap<String, Entity<InputState>>,
    input_freeform: HashMap<String, Entity<InputState>>,
    input_selections: HashMap<String, Vec<String>>,
    input_booleans: HashMap<String, bool>,
    selected_changeset: Option<String>,
    changeset_focus: HashMap<String, FocusHandle>,
    changeset_param_focus: HashMap<String, FocusHandle>,
    changeset_params: HashMap<String, String>,
    live_file_focus: HashMap<String, FocusHandle>,
    live_review_focus: HashMap<String, FocusHandle>,
    snapshot_refresh_focus: FocusHandle,
    preview_control_focus: FocusHandle,
    preview_close_focus: FocusHandle,
    drawer: Option<NodeKey>,
    drawer_close_focus: FocusHandle,
    drawer_return_focus: Option<FocusHandle>,
    approval_expanded: bool,
    approval_focus: FocusHandle,
    approval_second_focus: FocusHandle,
    session_tab: SessionTab,
    session_tab_focus: [FocusHandle; 3],
    file_preview: bool,
    file_focus: FocusHandle,
    facets: HashMap<NodeKey, usize>,
    hovered_facet: Option<(NodeKey, usize)>,
    focused: Option<NodeKey>,
    root_focus: FocusHandle,
    topology_focus: FocusHandle,
    facet_focus: [FocusHandle; 3],
    topology_scroll: ScrollHandle,
    topology_drag: Option<(Point<Pixels>, Point<Pixels>)>,
    lightfield: Option<LightfieldCache>,
    pulse: Option<PulseSignal>,
}

/// One rendered light field, kept until the geometry it depends on changes.
struct LightfieldCache {
    key: u64,
    image: Arc<RenderImage>,
    width: f32,
    height: f32,
}

/// A host-confirmed turn completion, shown for its bounded window.
struct PulseSignal {
    host: String,
    session: String,
    started: Instant,
}

fn input_question_key(request_id: &str, question_id: &str) -> String {
    format!("{request_id}\0{question_id}")
}

fn input_option_key(request_id: &str, question_id: &str, option_id: &str) -> String {
    format!("{request_id}\0{question_id}\0{option_id}")
}

/// Mirror JavaScript `Number(text)` for numeric inputs: surrounding
/// whitespace is ignored, `0x`/`0b`/`0o` prefixes parse as hex/binary/octal
/// (a sign prefix makes them NaN, like the web), and anything unparseable
/// yields NaN so the caller's finiteness check reports the source
/// `Invalid …` literal. Empty/whitespace-only yields 0, matching
/// `Number("")`; callers branch genuinely empty input before parsing.
fn js_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    let (digits, radix) = if let Some(digits) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        (digits, 16)
    } else if let Some(digits) = trimmed
        .strip_prefix("0b")
        .or_else(|| trimmed.strip_prefix("0B"))
    {
        (digits, 2)
    } else if let Some(digits) = trimmed
        .strip_prefix("0o")
        .or_else(|| trimmed.strip_prefix("0O"))
    {
        (digits, 8)
    } else {
        return trimmed.parse::<f64>().unwrap_or(f64::NAN);
    };
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut value = 0.0;
    for byte in digits.bytes() {
        let Some(digit) = char::from(byte).to_digit(radix) else {
            return f64::NAN;
        };
        value = value * f64::from(radix) + f64::from(digit);
    }
    value
}

fn register_input_option_focus<'a>(
    focus: &mut HashMap<String, FocusHandle>,
    request_id: &str,
    question_id: &str,
    option_ids: impl IntoIterator<Item = &'a str>,
    keys: &mut Vec<String>,
    cx: &mut Context<Relay>,
) {
    for option_id in option_ids {
        let option_key = input_option_key(request_id, question_id, option_id);
        keys.push(option_key.clone());
        focus
            .entry(option_key)
            .or_insert_with(|| cx.focus_handle().tab_stop(true));
    }
}

fn input_answer_value(
    answer: Option<&ahp::ahp_types::state::ChatInputAnswer>,
) -> Option<&ahp::ahp_types::state::ChatInputAnswerValue> {
    use ahp::ahp_types::state::ChatInputAnswer;
    match answer {
        Some(ChatInputAnswer::Draft(answer)) | Some(ChatInputAnswer::Submitted(answer)) => {
            Some(&answer.value)
        }
        _ => None,
    }
}

fn input_answer_text(answer: Option<&ahp::ahp_types::state::ChatInputAnswer>) -> Option<String> {
    use ahp::ahp_types::state::ChatInputAnswerValue;
    match input_answer_value(answer) {
        Some(ChatInputAnswerValue::Text(value)) => Some(value.value.clone()),
        Some(ChatInputAnswerValue::Number(value)) => Some(value.value.to_string()),
        _ => None,
    }
}

fn input_answer_bool(answer: Option<&ahp::ahp_types::state::ChatInputAnswer>) -> Option<bool> {
    use ahp::ahp_types::state::ChatInputAnswerValue;
    match input_answer_value(answer) {
        Some(ChatInputAnswerValue::Boolean(value)) => Some(value.value),
        _ => None,
    }
}

fn input_answer_selections(
    answer: Option<&ahp::ahp_types::state::ChatInputAnswer>,
) -> Option<Vec<String>> {
    use ahp::ahp_types::state::ChatInputAnswerValue;
    match input_answer_value(answer) {
        Some(ChatInputAnswerValue::Selected(value)) => Some(vec![value.value.clone()]),
        Some(ChatInputAnswerValue::SelectedMany(value)) => Some(value.value.clone()),
        _ => None,
    }
}

// The `Unknown` question variant preserves the raw host JSON. The web renders
// unknown kinds as plain text inputs (`session.tsx` falls through to the text
// branch) and reads them as text, so these accessors expose the same fields
// the web reads off `q` (`id`, `kind`, `title || message`, `required`,
// `defaultValue`) for the native text-input mirror.
// A missing field mirrors the web's JS `undefined`: interpolated as the
// literal `"undefined"`, rendered as empty. The accessors below return the
// raw field; the `"undefined"` fallback is applied at interpolation sites.
fn unknown_question_id(value: &serde_json::Value) -> &str {
    value
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("undefined")
}

fn unknown_question_kind(value: &serde_json::Value) -> &str {
    value
        .get("kind")
        .and_then(|kind| kind.as_str())
        .unwrap_or("undefined")
}

fn unknown_question_title(value: &serde_json::Value) -> Option<&str> {
    value
        .get("title")
        .and_then(|title| title.as_str())
        .filter(|title| !title.is_empty())
}

fn unknown_question_message(value: &serde_json::Value) -> &str {
    value
        .get("message")
        .and_then(|message| message.as_str())
        .unwrap_or_default()
}

fn unknown_question_required(value: &serde_json::Value) -> bool {
    value
        .get("required")
        .and_then(|required| required.as_bool())
        .unwrap_or(false)
}

fn unknown_question_default(value: &serde_json::Value) -> String {
    // Mirror the web text input's `defaultValue == null ? "" : String(...)`.
    match value.get("defaultValue") {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

fn input_answer_freeform(
    answer: Option<&ahp::ahp_types::state::ChatInputAnswer>,
) -> Option<Vec<String>> {
    use ahp::ahp_types::state::ChatInputAnswerValue;
    match input_answer_value(answer) {
        Some(ChatInputAnswerValue::Selected(value)) => value.freeform_values.clone(),
        Some(ChatInputAnswerValue::SelectedMany(value)) => value.freeform_values.clone(),
        _ => None,
    }
}

fn changeset_parameter_key(template_uri: &str, name: &str) -> String {
    format!("{template_uri}\0{name}")
}

fn changeset_turn_label(snapshot: &live::SessionSnapshot, index: usize, id: &str) -> String {
    let message = snapshot
        .changeset_turns
        .iter()
        .find(|(turn_id, _)| turn_id == id)
        .map(|(_, message)| {
            let message = message.trim();
            if message.is_empty() {
                id.to_owned()
            } else if message.chars().count() > 64 {
                format!("{}…", message.chars().take(63).collect::<String>())
            } else {
                message.to_owned()
            }
        })
        .unwrap_or_else(|| id.to_owned());
    format!("{} · {message}", index + 1)
}

fn changeset_parameter_label(name: &str) -> &str {
    match name {
        "turnId" => "Turn",
        "originalTurnId" => "From",
        "modifiedTurnId" => "To",
        _ => name,
    }
}

fn default_changeset_parameter(snapshot: &live::SessionSnapshot, name: &str) -> Option<String> {
    match name {
        "turnId" | "modifiedTurnId" => snapshot.changeset_turns.last().map(|(id, _)| id.clone()),
        "originalTurnId" => snapshot
            .changeset_turns
            .iter()
            .rev()
            .nth(1)
            .map(|(id, _)| id.clone()),
        _ => None,
    }
}

impl Relay {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let root_focus = cx.focus_handle();
        window.focus(&root_focus);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search fleet /"));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let connect_label = cx.new(|cx| InputState::new(window, cx).placeholder("Label"));
        let connect_endpoint =
            cx.new(|cx| InputState::new(window, cx).placeholder("wss://host.example"));
        let connect_token = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Access token")
                .masked(true)
        });
        let live_prompt =
            cx.new(|cx| InputState::new(window, cx).placeholder("Describe the work to perform…"));
        cx.observe(&live_prompt, |_, _, cx| cx.notify()).detach();
        let create_directory = cx.new(|cx| {
            InputState::new(window, cx).placeholder("file:///path/to/project")
        });
        cx.observe(&create_directory, |_, _, cx| cx.notify())
            .detach();
        let (live_fleet, live_error) = match live::LiveFleet::start() {
            Ok(fleet) => (Some(fleet), None),
            Err(_) => (
                None,
                Some("Host connection worker could not start.".to_owned()),
            ),
        };
        cx.spawn_in(window, async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(200)).await;
                if !matches!(
                    cx.update(|window, cx| this.update(cx, |this, cx| this.poll_live(window, cx))),
                    Ok(Ok(()))
                ) {
                    break;
                }
            }
        })
        .detach();
        Self {
            hosts_expanded: false,
            hosts_toggle_focus: cx.focus_handle().tab_stop(true),
            host_focus: example::hosts()
                .map(|(id, _)| (id, cx.focus_handle().tab_stop(true)))
                .collect(),
            hovered_host: None,
            active_host: None,
            sessions_expanded: false,
            sessions_toggle_focus: cx.focus_handle().tab_stop(true),
            catalog_focus: example::sessions()
                .map(|_| cx.focus_handle().tab_stop(true))
                .collect(),
            search,
            clear_search_focus: cx.focus_handle().tab_stop(true),
            live: false,
            live_fleet,
            live_hosts: Vec::new(),
            live_host_focus: HashMap::new(),
            live_session_focus: HashMap::new(),
            live_remove_focus: HashMap::new(),
            live_reconnect_focus: HashMap::new(),
            live_model_focus: HashMap::new(),
            create_agent_focus: HashMap::new(),
            live_model: None,
            live_prompt,
            queue_turn_focus: cx.focus_handle().tab_stop(true),
            cancel_turn_focus: cx.focus_handle().tab_stop(true),
            restart_worker_focus: cx.focus_handle().tab_stop(true),
            pending_turn_cancellation: None,
            pending_turn_submission: None,
            pending_turn_page: None,
            pending_file_review: None,
            pending_tool_confirmation: None,
            pending_input_submission: None,
            pending_session_disposal: None,
            pending_host_action: None,
            live_error,
            connect_open: false,
            connect_pending: false,
            connect_label,
            connect_endpoint,
            connect_token,
            connect_button_focus: cx.focus_handle().tab_stop(true),
            connect_close_focus: cx.focus_handle().tab_stop(true),
            connect_submit_focus: cx.focus_handle().tab_stop(true),
            create_open: false,
            create_pending: false,
            create_request: 0,
            create_provider: None,
            create_directory,
            create_trust: false,
            create_trust_focus: cx.focus_handle().tab_stop(true),
            create_config: None,
            create_config_directory: None,
            create_config_inputs: HashMap::new(),
            create_config_enums: HashMap::new(),
            create_config_bools: HashMap::new(),
            create_config_option_focus: HashMap::new(),
            create_refresh_focus: cx.focus_handle().tab_stop(true),
            create_submit_focus: cx.focus_handle().tab_stop(true),
            dispose_confirm_open: false,
            dispose_request: 0,
            dispose_focus: cx.focus_handle().tab_stop(true),
            dispose_confirm_focus: cx.focus_handle().tab_stop(true),
            dispose_cancel_focus: cx.focus_handle().tab_stop(true),
            mode_focus: cx.focus_handle().tab_stop(true),
            shortcuts_open: false,
            shortcuts_button_focus: cx.focus_handle().tab_stop(true),
            shortcuts_close_focus: cx.focus_handle().tab_stop(true),
            shortcuts_return_focus: None,
            status_open: false,
            status_button_focus: cx.focus_handle().tab_stop(true),
            status_close_focus: cx.focus_handle().tab_stop(true),
            status_dismiss_focus: cx.focus_handle().tab_stop(true),
            banner_dismiss_focus: cx.focus_handle().tab_stop(true),
            status_return_focus: None,
            selected: None,
            live_preview: None,
            snapshot_request: 0,
            session_snapshot: None,
            pending_file_preview: None,
            selected_chat: None,
            chat_focus: HashMap::new(),
            input_focus: HashMap::new(),
            input_option_focus: HashMap::new(),
            input_inputs: HashMap::new(),
            input_freeform: HashMap::new(),
            input_selections: HashMap::new(),
            input_booleans: HashMap::new(),
            selected_changeset: None,
            changeset_focus: HashMap::new(),
            changeset_param_focus: HashMap::new(),
            changeset_params: HashMap::new(),
            live_file_focus: HashMap::new(),
            live_review_focus: HashMap::new(),
            snapshot_refresh_focus: cx.focus_handle().tab_stop(true),
            preview_control_focus: cx.focus_handle().tab_stop(true),
            preview_close_focus: cx.focus_handle().tab_stop(true),
            drawer: None,
            drawer_close_focus: cx.focus_handle().tab_stop(true),
            drawer_return_focus: None,
            approval_expanded: false,
            approval_focus: cx.focus_handle().tab_stop(true),
            approval_second_focus: cx.focus_handle().tab_stop(true),
            session_tab: SessionTab::Conversation,
            session_tab_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            file_preview: false,
            file_focus: cx.focus_handle().tab_stop(true),
            facets: HashMap::new(),
            hovered_facet: None,
            focused: None,
            root_focus,
            topology_focus: cx.focus_handle().tab_stop(true),
            facet_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            topology_scroll: ScrollHandle::new(),
            topology_drag: None,
            lightfield: None,
            pulse: None,
        }
    }

    fn poll_live(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Some(fleet) = &self.live_fleet {
            match fleet.try_event() {
                Ok(live::Event::Hosts(hosts)) => {
                    if self.live
                        && let Some(NodeKey::Session(id, _)) = &self.drawer
                        && !hosts.iter().any(|host| {
                            &host.id == id
                                && host.status == "connected"
                                && self.live_hosts.iter().any(|previous| {
                                    &previous.id == id && previous.generation == host.generation
                                })
                        })
                    {
                        if self
                            .chat_focus
                            .values()
                            .any(|focus| focus.is_focused(window))
                        {
                            self.snapshot_refresh_focus.focus(window);
                        }
                        if self
                            .changeset_focus
                            .values()
                            .any(|focus| focus.is_focused(window))
                        {
                            self.session_tab_focus[1].focus(window);
                        }
                        if self
                            .live_file_focus
                            .values()
                            .any(|focus| focus.is_focused(window))
                        {
                            self.session_tab_focus[1].focus(window);
                        }
                        self.snapshot_request = self.snapshot_request.wrapping_add(1);
                        fleet.invalidate_snapshot();
                        self.pending_file_preview = None;
                        self.session_snapshot = Some(Err(
                            "Host disconnected; refresh the snapshot after reconnecting.",
                        ));
                    }
                    if self.live && self.drawer.is_none() && self.sessions_expanded {
                        for (key, focus) in &self.live_session_focus {
                            if let NodeKey::Session(id, uri) = key
                                && focus.is_focused(window)
                                && !hosts.iter().any(|host| {
                                    &host.id == id
                                        && host.sessions.iter().any(|session| &session.uri == uri)
                                })
                            {
                                self.sessions_toggle_focus.focus(window);
                            }
                        }
                    }
                    if self.live
                        && self.drawer.is_none()
                        && self.preview_control_focus.is_focused(window)
                        && self.selected.as_ref().is_some_and(|key| match key {
                            NodeKey::Host(id) | NodeKey::Provider(id, _) => {
                                !hosts.iter().any(|host| &host.id == id)
                            }
                            NodeKey::Session(id, uri) => !hosts.iter().any(|host| {
                                &host.id == id
                                    && host.sessions.iter().any(|session| &session.uri == uri)
                            }),
                        })
                    {
                        self.preview_close_focus.focus(window);
                    }
                    let finished = self.pending_host_action.as_ref().is_some_and(|pending| {
                        pending.acknowledged
                            && match pending.action {
                                live::HostAction::Remove => {
                                    !hosts.iter().any(|host| host.id == pending.id)
                                }
                                live::HostAction::Reconnect => hosts
                                    .iter()
                                    .find(|host| host.id == pending.id)
                                    .is_none_or(|host| {
                                        host.status != "connected"
                                            || host.generation > pending.generation
                                    }),
                            }
                    });
                    if finished {
                        self.pending_host_action = None;
                    }
                    let session_disposal_finished = self
                        .pending_session_disposal
                        .as_ref()
                        .is_some_and(|pending| {
                            pending.acknowledged
                                && !hosts.iter().any(|host| {
                                    host.id == pending.host
                                        && host
                                            .sessions
                                            .iter()
                                            .any(|session| session.uri == pending.session)
                                })
                        });
                    if session_disposal_finished {
                        self.pending_session_disposal = None;
                        self.dispose_confirm_open = false;
                        self.close_drawer(window);
                    }
                    let selected_model_invalid =
                        self.live_model.as_deref().is_some_and(|model_id| {
                            let Some(NodeKey::Session(host_id, _)) = &self.drawer else {
                                return false;
                            };
                            let Some(snapshot) = self
                                .session_snapshot
                                .as_ref()
                                .and_then(|result| result.as_ref().ok())
                            else {
                                return false;
                            };
                            let Some(host) = hosts
                                .iter()
                                .find(|host| &host.id == host_id && host.status == "connected")
                            else {
                                return true;
                            };
                            let Some(agent) = host
                                .agents
                                .iter()
                                .find(|agent| agent.provider == snapshot.provider)
                            else {
                                return true;
                            };
                            agent
                                .models
                                .iter()
                                .find(|model| model.id == model_id)
                                .is_none_or(|model| model.disabled)
                        });
                    if selected_model_invalid {
                        self.live_model = None;
                    }
                    self.live_host_focus
                        .retain(|id, _| hosts.iter().any(|host| &host.id == id));
                    self.live_remove_focus
                        .retain(|id, _| hosts.iter().any(|host| &host.id == id));
                    self.live_reconnect_focus
                        .retain(|id, _| hosts.iter().any(|host| &host.id == id));
                    self.live_model_focus.retain(|key, _| {
                        hosts.iter().any(|host| {
                            host.agents.iter().any(|agent| {
                                key == &live_model_focus_key(
                                    &host.id,
                                    &agent.provider,
                                    DEFAULT_MODEL_ID,
                                ) || agent.models.iter().any(|model| {
                                    key == &live_model_focus_key(
                                        &host.id,
                                        &agent.provider,
                                        &model.id,
                                    )
                                })
                            })
                        })
                    });
                    self.create_agent_focus.retain(|key, _| {
                        hosts.iter().any(|host| {
                            host.agents.iter().any(|agent| {
                                key == &create_agent_focus_key(&host.id, &agent.provider)
                            })
                        })
                    });
                    self.live_session_focus.retain(|key, _| match key {
                        NodeKey::Session(id, uri) => hosts.iter().any(|host| {
                            &host.id == id
                                && host.sessions.iter().any(|session| &session.uri == uri)
                        }),
                        _ => false,
                    });
                    for host in &hosts {
                        for session in &host.sessions {
                            self.live_session_focus
                                .entry(NodeKey::Session(host.id.clone(), session.uri.clone()))
                                .or_insert_with(|| cx.focus_handle().tab_stop(true));
                        }
                        self.live_host_focus
                            .entry(host.id.clone())
                            .or_insert_with(|| cx.focus_handle().tab_stop(true));
                        self.live_remove_focus
                            .entry(host.id.clone())
                            .or_insert_with(|| cx.focus_handle().tab_stop(true));
                        self.live_reconnect_focus
                            .entry(host.id.clone())
                            .or_insert_with(|| cx.focus_handle().tab_stop(true));
                        for agent in &host.agents {
                            self.create_agent_focus
                                .entry(create_agent_focus_key(&host.id, &agent.provider))
                                .or_insert_with(|| cx.focus_handle().tab_stop(true));
                            self.live_model_focus
                                .entry(live_model_focus_key(
                                    &host.id,
                                    &agent.provider,
                                    DEFAULT_MODEL_ID,
                                ))
                                .or_insert_with(|| cx.focus_handle().tab_stop(true));
                            for model in &agent.models {
                                self.live_model_focus
                                    .entry(live_model_focus_key(
                                        &host.id,
                                        &agent.provider,
                                        &model.id,
                                    ))
                                    .or_insert_with(|| cx.focus_handle().tab_stop(true));
                            }
                        }
                    }
                    // The console reads a session's conversation only when it
                    // asks for one, so a host-confirmed turn end would go
                    // unnoticed while it sits idle. The summary the host pushes
                    // carries the session's activity, so a change there re-asks
                    // for the open session and the pulse can fire.
                    let watched = match &self.drawer {
                        Some(NodeKey::Session(host, uri)) => {
                            let next = session_summary(&hosts, host, uri);
                            (next.is_some() && next != session_summary(&self.live_hosts, host, uri))
                                .then(|| NodeKey::Session(host.clone(), uri.clone()))
                        }
                        _ => None,
                    };
                    self.live_hosts = hosts;
                    changed = true;
                    if let Some(key) = watched {
                        self.request_snapshot(
                            &key,
                            live::ChatSelection::Preferred(self.selected_chat.clone()),
                            live::ChangesetSelection::Preferred(self.selected_changeset.clone()),
                        );
                    }
                }

                Ok(live::Event::SessionSnapshot {
                    request,
                    host,
                    uri,
                    generation,
                    result,
                }) => {
                    if self.live
                        && request == self.snapshot_request
                        && self.drawer.as_ref()
                            == Some(&NodeKey::Session(host.clone(), uri.clone()))
                    {
                        let connected = self.live_hosts.iter().any(|item| {
                            item.id == host
                                && item.generation == generation
                                && item.status == "connected"
                        });
                        let result = if connected {
                            result.map(|snapshot| *snapshot)
                        } else {
                            Err("Host connection changed; reopen the session.")
                        };
                        let pending_file = self.pending_file_preview.take();
                        if connected
                            && let (Some(file), Err(error), Some(Ok(snapshot))) =
                                (&pending_file, &result, &mut self.session_snapshot)
                        {
                            snapshot.preview = Some((file.clone(), Err(*error)));
                            changed = true;
                            continue;
                        }
                        // The retained snapshot is the baseline: asking for a
                        // background refresh leaves it in place, while a
                        // disconnect, a failed read and a closed drawer all
                        // reset it, so no pulse can cross one of those gaps.
                        let completed = pulse_baseline(self.session_snapshot.as_ref())
                            .zip(result.as_ref().ok())
                            .is_some_and(|(previous, next)| turn_just_completed(previous, next));
                        if completed {
                            self.start_lightfield_pulse(host.clone(), uri.clone(), window, cx);
                        }
                        if let Ok(snapshot) = &result {
                            self.selected_chat = snapshot.selected_chat.clone();
                            if snapshot.changes.is_ok() {
                                self.selected_changeset = snapshot.selected_changeset.clone();
                            }
                            self.changeset_focus.retain(|uri, _| {
                                snapshot
                                    .available_changesets
                                    .iter()
                                    .any(|(id, _)| id == uri)
                            });
                            self.changeset_param_focus.clear();
                            self.changeset_params.retain(|key, _| {
                                snapshot.changeset_templates.iter().any(|(uri, template)| {
                                    key.starts_with(&format!("{uri}\0"))
                                        && live::changeset_parameter_names(template)
                                            .iter()
                                            .any(|name| key == &changeset_parameter_key(uri, name))
                                })
                            });
                            for (uri, _) in &snapshot.available_changesets {
                                self.changeset_focus
                                    .entry(uri.clone())
                                    .or_insert_with(|| cx.focus_handle().tab_stop(true));
                            }
                            for (uri, template) in &snapshot.changeset_templates {
                                for name in live::changeset_parameter_names(template) {
                                    self.changeset_param_focus
                                        .entry(changeset_parameter_key(uri, name))
                                        .or_insert_with(|| cx.focus_handle().tab_stop(true));
                                }
                            }
                            let files = snapshot
                                .changes
                                .as_ref()
                                .ok()
                                .and_then(|changes| changes.as_ref());
                            if self.live_file_focus.iter().any(|(id, focus)| {
                                focus.is_focused(window)
                                    && !files.is_some_and(|(_, changes)| {
                                        changes.files.iter().any(|file| &file.id == id)
                                    })
                            }) {
                                self.session_tab_focus[1].focus(window);
                            }
                            self.live_file_focus.retain(|id, _| {
                                files.is_some_and(|(_, changes)| {
                                    changes.files.iter().any(|file| &file.id == id)
                                })
                            });
                            self.live_review_focus.retain(|id, _| {
                                files.is_some_and(|(_, changes)| {
                                    snapshot.changeset_reviewable
                                        && changes.files.iter().any(|file| &file.id == id)
                                })
                            });
                            if let Some((_, changes)) = files {
                                for file in &changes.files {
                                    self.live_file_focus
                                        .entry(file.id.clone())
                                        .or_insert_with(|| cx.focus_handle().tab_stop(true));
                                    if snapshot.changeset_reviewable {
                                        self.live_review_focus
                                            .entry(file.id.clone())
                                            .or_insert_with(|| cx.focus_handle().tab_stop(true));
                                    }
                                }
                            }
                            self.chat_focus.retain(|uri, _| {
                                snapshot.visible_chats.iter().any(|(id, _)| id == uri)
                            });
                            for (uri, _) in &snapshot.visible_chats {
                                self.chat_focus
                                    .entry(uri.clone())
                                    .or_insert_with(|| cx.focus_handle().tab_stop(true));
                            }
                            let mut input_keys = Vec::new();
                            let mut input_option_keys = Vec::new();
                            for request in &snapshot.input_requests {
                                let answers = request.request.answers.as_ref();
                                for question in request.request.questions.as_deref().unwrap_or(&[])
                                {
                                    let (question_id, prompt): (&str, &str) = match question {
                                        ahp::ahp_types::state::ChatInputQuestion::Text(
                                            question,
                                        ) => (&question.id, question.message.as_str()),
                                        ahp::ahp_types::state::ChatInputQuestion::Number(
                                            question,
                                        )
                                        | ahp::ahp_types::state::ChatInputQuestion::Integer(
                                            question,
                                        ) => (&question.id, question.message.as_str()),
                                        ahp::ahp_types::state::ChatInputQuestion::Boolean(
                                            question,
                                        ) => (&question.id, question.message.as_str()),
                                        ahp::ahp_types::state::ChatInputQuestion::SingleSelect(
                                            question,
                                        ) => (&question.id, question.message.as_str()),
                                        ahp::ahp_types::state::ChatInputQuestion::MultiSelect(
                                            question,
                                        ) => (&question.id, question.message.as_str()),
                                        ahp::ahp_types::state::ChatInputQuestion::Unknown(
                                            value,
                                        ) => (
                                            unknown_question_id(value),
                                            unknown_question_message(value),
                                        ),
                                    };
                                    let prompt = prompt.to_owned();
                                    let key = input_question_key(&request.request.id, question_id);
                                    input_keys.push(key.clone());
                                    self.input_focus
                                        .entry(key.clone())
                                        .or_insert_with(|| cx.focus_handle().tab_stop(true));
                                    match question {
                                        ahp::ahp_types::state::ChatInputQuestion::SingleSelect(
                                            question,
                                        ) => register_input_option_focus(
                                            &mut self.input_option_focus,
                                            &request.request.id,
                                            question_id,
                                            question
                                                .options
                                                .iter()
                                                .map(|option| option.id.as_str()),
                                            &mut input_option_keys,
                                            cx,
                                        ),
                                        ahp::ahp_types::state::ChatInputQuestion::MultiSelect(
                                            question,
                                        ) => register_input_option_focus(
                                            &mut self.input_option_focus,
                                            &request.request.id,
                                            question_id,
                                            question
                                                .options
                                                .iter()
                                                .map(|option| option.id.as_str()),
                                            &mut input_option_keys,
                                            cx,
                                        ),
                                        _ => {}
                                    }
                                    let answer =
                                        answers.and_then(|answers| answers.get(question_id));
                                    match question {
                                        ahp::ahp_types::state::ChatInputQuestion::Text(
                                            question,
                                        ) => {
                                            let default = input_answer_text(answer)
                                                .or_else(|| question.default_value.clone())
                                                .unwrap_or_default();
                                            if !self.input_inputs.contains_key(&key) {
                                                let input = cx.new(|cx| {
                                                    InputState::new(window, cx)
                                                        .placeholder(prompt.clone())
                                                });
                                                input.update(cx, |input, cx| {
                                                    input.set_value(default, window, cx)
                                                });
                                                cx.observe(&input, |_, _, cx| cx.notify()).detach();
                                                self.input_inputs.insert(key.clone(), input);
                                            }
                                        }
                                        ahp::ahp_types::state::ChatInputQuestion::Number(
                                            question,
                                        )
                                        | ahp::ahp_types::state::ChatInputQuestion::Integer(
                                            question,
                                        ) => {
                                            let default = input_answer_text(answer)
                                                .or_else(|| {
                                                    question
                                                        .default_value
                                                        .map(|value| value.to_string())
                                                })
                                                .unwrap_or_default();
                                            if !self.input_inputs.contains_key(&key) {
                                                let input = cx.new(|cx| {
                                                    InputState::new(window, cx)
                                                        .placeholder(prompt.clone())
                                                });
                                                input.update(cx, |input, cx| {
                                                    input.set_value(default, window, cx)
                                                });
                                                cx.observe(&input, |_, _, cx| cx.notify()).detach();
                                                self.input_inputs.insert(key.clone(), input);
                                            }
                                        }
                                        ahp::ahp_types::state::ChatInputQuestion::Boolean(
                                            question,
                                        ) => {
                                            // Mirror the web select: with no prior
                                            // answer and no default the question stays
                                            // unanswered (`Choose an answer`), so submit
                                            // can skip it or report `Complete …`
                                            // instead of sending `false`.
                                            if let Some(default) =
                                                input_answer_bool(answer).or(question.default_value)
                                            {
                                                self.input_booleans.entry(key).or_insert(default);
                                            }
                                        }
                                        ahp::ahp_types::state::ChatInputQuestion::SingleSelect(
                                            question,
                                        ) => {
                                            self.input_selections
                                                .entry(key.clone())
                                                .or_insert_with(|| {
                                                    input_answer_selections(answer)
                                                        .unwrap_or_default()
                                                });
                                            if question.allow_freeform_input.unwrap_or(false) {
                                                let default = input_answer_freeform(answer)
                                                    .and_then(|values| values.into_iter().next())
                                                    .unwrap_or_default();
                                                if !self.input_freeform.contains_key(&key) {
                                                    let input = cx.new(|cx| {
                                                        InputState::new(window, cx)
                                                            .placeholder("Other response")
                                                    });
                                                    input.update(cx, |input, cx| {
                                                        input.set_value(default, window, cx)
                                                    });
                                                    cx.observe(&input, |_, _, cx| cx.notify())
                                                        .detach();
                                                    self.input_freeform.insert(key.clone(), input);
                                                }
                                            }
                                        }
                                        ahp::ahp_types::state::ChatInputQuestion::MultiSelect(
                                            question,
                                        ) => {
                                            self.input_selections
                                                .entry(key.clone())
                                                .or_insert_with(|| {
                                                    input_answer_selections(answer)
                                                        .unwrap_or_default()
                                                });
                                            if question.allow_freeform_input.unwrap_or(false) {
                                                let default = input_answer_freeform(answer)
                                                    .and_then(|values| values.into_iter().next())
                                                    .unwrap_or_default();
                                                if !self.input_freeform.contains_key(&key) {
                                                    let input = cx.new(|cx| {
                                                        InputState::new(window, cx)
                                                            .placeholder("Other response")
                                                    });
                                                    input.update(cx, |input, cx| {
                                                        input.set_value(default, window, cx)
                                                    });
                                                    cx.observe(&input, |_, _, cx| cx.notify())
                                                        .detach();
                                                    self.input_freeform.insert(key.clone(), input);
                                                }
                                            }
                                        }
                                        ahp::ahp_types::state::ChatInputQuestion::Unknown(
                                            value,
                                        ) => {
                                            // Mirror the web text-input
                                            // fallthrough for unknown kinds.
                                            let default = input_answer_text(answer)
                                                .unwrap_or_else(|| unknown_question_default(value));
                                            if !self.input_inputs.contains_key(&key) {
                                                let input = cx.new(|cx| {
                                                    InputState::new(window, cx)
                                                        .placeholder(prompt.clone())
                                                });
                                                input.update(cx, |input, cx| {
                                                    input.set_value(default, window, cx)
                                                });
                                                cx.observe(&input, |_, _, cx| cx.notify()).detach();
                                                self.input_inputs.insert(key.clone(), input);
                                            }
                                        }
                                    }
                                }
                            }
                            self.input_focus
                                .retain(|key, _| input_keys.iter().any(|item| item == key));
                            self.input_option_focus
                                .retain(|key, _| input_option_keys.iter().any(|item| item == key));
                            self.input_inputs
                                .retain(|key, _| input_keys.iter().any(|item| item == key));
                            self.input_freeform
                                .retain(|key, _| input_keys.iter().any(|item| item == key));
                            self.input_selections
                                .retain(|key, _| input_keys.iter().any(|item| item == key));
                            self.input_booleans
                                .retain(|key, _| input_keys.iter().any(|item| item == key));
                        }
                        let cancellation_confirmed = self
                            .pending_turn_cancellation
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.host == host
                                    && pending.session == uri
                                    && pending.generation == generation
                                    && result.as_ref().ok().and_then(|snapshot| {
                                        snapshot
                                            .conversation
                                            .as_ref()
                                            .ok()
                                            .and_then(|chat| chat.as_ref())
                                            .map(|chat| {
                                                chat.active_turn
                                                    .as_ref()
                                                    .is_none_or(|turn| turn.id != pending.turn_id)
                                            })
                                    }) == Some(true)
                            });
                        if cancellation_confirmed {
                            self.pending_turn_cancellation = None;
                        }
                        self.session_snapshot = Some(result);
                        self.load_files_if_needed();
                        changed = true;
                    }
                }
                Ok(live::Event::AddHost { error }) => {
                    if self.connect_pending {
                        self.connect_pending = false;
                        if let Some(error) = error {
                            self.live_error = Some(error.to_owned());
                        } else {
                            self.connect_label
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            self.connect_endpoint
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            if self.connect_open {
                                self.close_connect(window, cx);
                            }
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::HostAction { id, action, error }) => {
                    if let Some(pending) = &mut self.pending_host_action
                        && pending.id == id
                        && pending.action == action
                    {
                        if let Some(error) = error {
                            self.pending_host_action = None;
                            self.live_error = Some(error.to_owned());
                        } else {
                            pending.acknowledged = true;
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::SessionConfigResolved {
                    request,
                    host,
                    generation,
                    provider,
                    directory,
                    result,
                }) => {
                    let current_host = self.live_hosts.iter().find(|item| item.id == host);
                    let current_directory = self.create_directory.read(cx).value();
                    let request_matches = self.create_open
                        && request == self.create_request
                        && self.create_provider.as_deref() == Some(provider.as_str());
                    if request_matches && current_directory != directory {
                        self.live_error =
                            Some("Workspace changed · refresh host options".to_owned());
                        changed = true;
                    } else if request_matches
                        && current_directory == directory
                        && current_host.is_some_and(|item| {
                            item.generation == generation && item.status == "connected"
                        })
                    {
                        match result {
                            Ok(config) => {
                                self.create_config_inputs.clear();
                                self.create_config_enums.clear();
                                self.create_config_bools.clear();
                                self.create_config_option_focus.clear();
                                for (name, property) in &config.schema.properties {
                                    if property.read_only == Some(true) {
                                        continue;
                                    }
                                    // Mirror the web ConfigField: types outside
                                    // string/number/boolean/array/object render
                                    // as static host-managed rows with no input.
                                    if !matches!(
                                        property.r#type.as_str(),
                                        "string" | "number" | "boolean" | "array" | "object"
                                    ) {
                                        continue;
                                    }
                                    let value =
                                        config.values.get(name).or(property.default.as_ref());
                                    // Mirror helpers.readConfig: a static enum
                                    // selects by index; plain booleans select
                                    // Yes/No. Dynamic enums stay free text.
                                    if config_is_static_enum(property) {
                                        let Some(choices) = property.r#enum.as_ref() else {
                                            continue;
                                        };
                                        let selected = value.and_then(|value| {
                                            choices.iter().position(|choice| choice == value)
                                        });
                                        self.create_config_enums.insert(name.clone(), selected);
                                        for index in 0..choices.len() {
                                            let key =
                                                create_config_option_key(name, &index.to_string());
                                            self.create_config_option_focus
                                                .entry(key)
                                                .or_insert_with(|| {
                                                    cx.focus_handle().tab_stop(true)
                                                });
                                        }
                                        self.create_config_option_focus
                                            .entry(create_config_option_key(name, "default"))
                                            .or_insert_with(|| cx.focus_handle().tab_stop(true));
                                        continue;
                                    }
                                    if property.r#type == "boolean" {
                                        let selected = value.and_then(|value| match value {
                                            serde_json::Value::Bool(selected) => Some(*selected),
                                            _ => None,
                                        });
                                        self.create_config_bools.insert(name.clone(), selected);
                                        for suffix in ["default", "true", "false"] {
                                            let key = create_config_option_key(name, suffix);
                                            self.create_config_option_focus
                                                .entry(key)
                                                .or_insert_with(|| {
                                                    cx.focus_handle().tab_stop(true)
                                                });
                                        }
                                        continue;
                                    }
                                    let input_value = config_input_value(value, property);
                                    let placeholder = if property.r#type == "array"
                                        && property
                                            .items
                                            .as_ref()
                                            .is_some_and(|items| items.r#type == "string")
                                    {
                                        "One entry per line"
                                    } else {
                                        ""
                                    };
                                    let input = cx.new(|cx| {
                                        InputState::new(window, cx).placeholder(placeholder)
                                    });
                                    input.update(cx, |input, cx| {
                                        input.set_value(input_value, window, cx)
                                    });
                                    cx.observe(&input, |_, _, cx| cx.notify()).detach();
                                    self.create_config_inputs.insert(name.clone(), input);
                                }
                                self.create_config = Some(*config);
                                self.create_config_directory = Some(directory);
                                self.live_error = None;
                            }
                            Err(error) => self.live_error = Some(error.to_owned()),
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::SessionCreated {
                    request,
                    host,
                    generation,
                    uri,
                    error,
                }) => {
                    if self.create_pending
                        && request == self.create_request
                        && self
                            .live_hosts
                            .iter()
                            .any(|item| item.id == host && item.generation == generation)
                    {
                        self.create_pending = false;
                        if let Some(error) = error {
                            self.live_error = Some(error.to_owned());
                        } else if let Some(uri) = uri {
                            self.live_error = None;
                            self.selected = None;
                            self.open_drawer(
                                NodeKey::Session(host, uri),
                                self.drawer_close_focus.clone(),
                                window,
                            );
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::SessionDisposed {
                    request,
                    host,
                    session,
                    generation,
                    error,
                }) => {
                    let matches_pending =
                        self.pending_session_disposal
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.request == request
                                    && pending.host == host
                                    && pending.session == session
                                    && pending.generation == generation
                            });
                    if matches_pending {
                        if let Some(error) = error {
                            self.pending_session_disposal = None;
                            self.live_error = Some(error.to_owned());
                        } else if let Some(pending) = &mut self.pending_session_disposal {
                            pending.acknowledged = true;
                            self.live_error = None;
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::TurnCancellation {
                    host,
                    session,
                    generation,
                    chat,
                    turn_id,
                    error,
                }) => {
                    let matches_pending =
                        self.pending_turn_cancellation
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.host == host
                                    && pending.session == session
                                    && pending.generation == generation
                                    && pending.chat == chat
                                    && pending.turn_id == turn_id
                            });
                    if matches_pending {
                        if let Some(error) = error {
                            self.pending_turn_cancellation = None;
                            self.live_error = Some(error.to_owned());
                        } else if let Some(key @ NodeKey::Session(_, _)) = self.drawer.clone() {
                            self.refresh_snapshot(
                                &key,
                                live::ChatSelection::Explicit(chat),
                                live::ChangesetSelection::Preferred(
                                    self.selected_changeset.clone(),
                                ),
                            );
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::TurnQueued {
                    host,
                    session,
                    generation,
                    chat,
                    message_id,
                    error,
                }) => {
                    let matches_pending =
                        self.pending_turn_submission
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.host == host
                                    && pending.session == session
                                    && pending.generation == generation
                                    && pending.chat == chat
                                    && pending.message_id == message_id
                            });
                    if matches_pending {
                        self.pending_turn_submission = None;
                        if let Some(error) = error {
                            self.live_error = Some(error.to_owned());
                        } else {
                            self.live_prompt
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            if let Some(key @ NodeKey::Session(_, _)) = self.drawer.clone() {
                                self.refresh_snapshot(
                                    &key,
                                    live::ChatSelection::Explicit(chat),
                                    live::ChangesetSelection::Preferred(
                                        self.selected_changeset.clone(),
                                    ),
                                );
                            }
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::ToolConfirmation {
                    host,
                    session,
                    generation,
                    chat,
                    turn_id,
                    tool_call_id,
                    error,
                }) => {
                    let matches_pending =
                        self.pending_tool_confirmation
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.host == host
                                    && pending.session == session
                                    && pending.generation == generation
                                    && pending.chat == chat
                                    && pending.turn_id == turn_id
                                    && pending.tool_call_id == tool_call_id
                            });
                    if matches_pending {
                        self.pending_tool_confirmation = None;
                        if let Some(error) = error {
                            self.live_error = Some(error.to_owned());
                        } else if let Some(key @ NodeKey::Session(_, _)) = self.drawer.clone() {
                            self.refresh_snapshot(
                                &key,
                                live::ChatSelection::Explicit(chat),
                                live::ChangesetSelection::Preferred(
                                    self.selected_changeset.clone(),
                                ),
                            );
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::InputAnswered {
                    host,
                    session,
                    generation,
                    request_id,
                    error,
                }) => {
                    let matches_pending = self.pending_input_submission.as_deref()
                        == Some(request_id.as_str())
                        && matches!(
                            &self.drawer,
                            Some(NodeKey::Session(drawer_host, drawer_session))
                                if drawer_host == &host && drawer_session == &session
                        )
                        && self
                            .live_hosts
                            .iter()
                            .any(|item| item.id == host && item.generation == generation);
                    if matches_pending {
                        self.pending_input_submission = None;
                        self.input_focus.clear();
                        self.input_option_focus.clear();
                        if let Some(error) = error {
                            self.live_error = Some(error.to_owned());
                        } else if let Some(key @ NodeKey::Session(_, _)) = self.drawer.clone() {
                            self.live_error = None;
                            self.refresh_snapshot(
                                &key,
                                live::ChatSelection::Preferred(self.selected_chat.clone()),
                                live::ChangesetSelection::Preferred(
                                    self.selected_changeset.clone(),
                                ),
                            );
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::FileReviewChanged {
                    host,
                    session,
                    generation,
                    changeset,
                    file,
                    reviewed,
                    error,
                }) => {
                    let matches_pending =
                        self.pending_file_review.as_ref().is_some_and(|pending| {
                            pending.host == host
                                && pending.session == session
                                && pending.generation == generation
                                && pending.changeset == changeset
                                && pending.file == file
                                && pending.reviewed == reviewed
                        });
                    if matches_pending {
                        self.pending_file_review = None;
                        if let Some(error) = error {
                            self.live_error = Some(error.to_owned());
                        } else if let Some(key @ NodeKey::Session(_, _)) = self.drawer.clone() {
                            self.live_error = None;
                            self.refresh_snapshot(
                                &key,
                                live::ChatSelection::Preferred(self.selected_chat.clone()),
                                live::ChangesetSelection::Explicit(changeset),
                            );
                        }
                        changed = true;
                    }
                }
                Ok(live::Event::TurnsLoaded {
                    host,
                    session,
                    generation,
                    chat,
                    cursor,
                    error,
                }) => {
                    let matches_pending = self.pending_turn_page.as_ref().is_some_and(|pending| {
                        pending.host == host
                            && pending.session == session
                            && pending.generation == generation
                            && pending.chat == chat
                            && pending.cursor == cursor
                    });
                    if matches_pending {
                        self.pending_turn_page = None;
                        if let Some(error) = error {
                            self.live_error = Some(error.to_owned());
                        } else if let Some(key @ NodeKey::Session(_, _)) = self.drawer.clone() {
                            self.live_error = None;
                            self.refresh_snapshot(
                                &key,
                                live::ChatSelection::Explicit(chat),
                                live::ChangesetSelection::Preferred(
                                    self.selected_changeset.clone(),
                                ),
                            );
                        }
                        changed = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if self
                        .chat_focus
                        .values()
                        .any(|focus| focus.is_focused(window))
                    {
                        self.snapshot_refresh_focus.focus(window);
                    }
                    if self
                        .changeset_focus
                        .values()
                        .any(|focus| focus.is_focused(window))
                    {
                        self.session_tab_focus[1].focus(window);
                    }
                    if self
                        .live_file_focus
                        .values()
                        .any(|focus| focus.is_focused(window))
                    {
                        self.session_tab_focus[1].focus(window);
                    }
                    if self
                        .live_review_focus
                        .values()
                        .any(|focus| focus.is_focused(window))
                    {
                        self.session_tab_focus[1].focus(window);
                    }
                    self.live_fleet = None;
                    self.pending_host_action = None;
                    self.pending_turn_cancellation = None;
                    self.pending_turn_page = None;
                    self.pending_file_review = None;
                    self.pending_tool_confirmation = None;
                    self.pending_input_submission = None;
                    self.input_focus.clear();
                    self.input_option_focus.clear();
                    self.connect_pending = false;
                    self.snapshot_request = self.snapshot_request.wrapping_add(1);
                    self.pending_file_preview = None;
                    self.session_snapshot = Some(Err("Host connection worker stopped."));
                    for host in &mut self.live_hosts {
                        host.status = "disconnected";
                    }
                    self.live_error = Some("Host connection worker stopped.".to_owned());
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn close_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.connect_open = false;
        self.connect_token
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.connect_button_focus.focus(window);
    }

    fn submit_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.connect_pending {
            return;
        }
        let result = self
            .live_fleet
            .as_ref()
            .ok_or("Host connection worker unavailable.")
            .and_then(|fleet| {
                fleet.add(
                    &self.connect_label.read(cx).value(),
                    &self.connect_endpoint.read(cx).value(),
                    &self.connect_token.read(cx).value(),
                )
            });
        match result {
            Ok(()) => {
                self.live_error = None;
                self.connect_pending = true;
                self.connect_token
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.connect_close_focus.focus(window);
            }
            Err(error) => self.live_error = Some(error.to_owned()),
        }
        cx.notify();
    }

    fn open_create_session(
        &mut self,
        host: String,
        provider: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let provider = if provider.is_empty() {
            self.live_hosts
                .iter()
                .find(|item| item.id == host && item.status == "connected")
                .and_then(|item| item.agents.first())
                .map(|agent| agent.provider.clone())
                .unwrap_or_default()
        } else {
            provider
        };
        self.create_open = true;
        self.create_pending = false;
        self.create_provider = Some(provider);
        self.create_config = None;
        self.create_config_directory = None;
        self.create_config_inputs.clear();
        self.create_config_enums.clear();
        self.create_config_bools.clear();
        self.create_config_option_focus.clear();
        self.create_trust = false;
        self.create_directory
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.live_error = None;
        self.request_create_config(&host, window, cx);
        self.create_directory
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn open_catalog_create_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.live_fleet.is_none() {
            self.live_error = Some("Host connection worker unavailable.".to_owned());
            cx.notify();
            return;
        }
        let Some(host) = self
            .active_host
            .as_deref()
            .and_then(|active| {
                self.live_hosts
                    .iter()
                    .find(|host| host.id == active && host.status == "connected")
            })
            .or_else(|| {
                self.live_hosts
                    .iter()
                    .find(|host| host.status == "connected")
            })
            .map(|host| host.id.clone())
        else {
            self.live_error = Some("No connected host is available.".to_owned());
            cx.notify();
            return;
        };
        self.sessions_expanded = false;
        self.selected = None;
        self.live_preview = None;
        self.open_drawer(
            NodeKey::Provider(host.clone(), String::new()),
            self.sessions_toggle_focus.clone(),
            window,
        );
        self.open_create_session(host, String::new(), window, cx);
    }

    fn create_config_payload(
        &self,
        cx: &Context<Self>,
    ) -> Result<ahp::ahp_types::common::JsonObject, String> {
        let mut values = self
            .create_config
            .as_ref()
            .map(|config| config.values.clone())
            .unwrap_or_default();
        let Some(config) = &self.create_config else {
            return Ok(values);
        };
        for (name, property) in &config.schema.properties {
            // Read-only and unsupported rows have no inputs; their values
            // stay exactly as the host resolved them.
            if property.read_only == Some(true) {
                continue;
            }
            let title = if property.title.is_empty() {
                name.clone()
            } else {
                property.title.clone()
            };
            // Mirror helpers.readConfig: a static enum maps a selected
            // index back to its value; an empty selection keeps the
            // previously resolved value.
            if config_is_static_enum(property) {
                let selected = self.create_config_enums.get(name).copied().flatten();
                let Some(index) = selected else {
                    continue;
                };
                let Some(choices) = property.r#enum.as_ref() else {
                    continue;
                };
                let choice = choices
                    .get(index)
                    .ok_or_else(|| format!("Invalid host option {title}"))?;
                values.insert(name.clone(), choice.clone());
                continue;
            }
            if property.r#type == "boolean" {
                let selected = self.create_config_bools.get(name).copied().flatten();
                let Some(selected) = selected else {
                    continue;
                };
                values.insert(name.clone(), serde_json::Value::Bool(selected));
                continue;
            }
            let Some(input) = self.create_config_inputs.get(name) else {
                continue;
            };
            // Mirror helpers.readConfig: only arrays whose items are strings
            // edit as one entry per line; every other array or object edits
            // as JSON.
            let kind = property.r#type.as_str();
            let string_items = kind == "array"
                && property
                    .items
                    .as_ref()
                    .is_some_and(|items| items.r#type == "string");
            let raw = input.read(cx).value();
            if kind == "object" || (kind == "array" && !string_items) {
                // An empty field keeps the previously resolved value.
                if raw.trim().is_empty() {
                    continue;
                }
                let parsed: serde_json::Value = serde_json::from_str(raw.trim())
                    .map_err(|_| format!("Invalid JSON for host option {title}"))?;
                let shape_ok = if kind == "object" {
                    parsed.is_object()
                } else {
                    parsed.is_array()
                };
                if !shape_ok {
                    return Err(format!("Invalid type for host option {title}"));
                }
                values.insert(name.clone(), parsed);
                continue;
            }
            if kind == "array" {
                values.insert(
                    name.clone(),
                    serde_json::Value::Array(
                        raw.split(['\r', '\n'])
                            .map(str::trim)
                            .filter(|line| !line.is_empty())
                            .map(|line| serde_json::Value::String(line.to_owned()))
                            .collect(),
                    ),
                );
                continue;
            }
            // An empty field keeps the previously resolved value.
            if raw.is_empty() {
                continue;
            }
            let value = match kind {
                // Mirror helpers.readConfig: `Number(text)` accepts
                // hex/binary/octal and maps whitespace-only to 0; a
                // non-finite result reports `Invalid host option …`.
                "number" => serde_json::Number::from_f64(js_number(raw.trim()))
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| format!("Invalid host option {title}"))?,
                _ => serde_json::Value::String(raw.to_string()),
            };
            values.insert(name.clone(), value);
        }
        Ok(values)
    }

    fn request_create_config(&mut self, host: &str, _: &mut Window, cx: &mut Context<Self>) {
        let Some(provider) = self
            .create_provider
            .clone()
            .filter(|provider| !provider.is_empty())
        else {
            return;
        };
        let Some(host_view) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
        else {
            self.live_error = Some("Host is not connected".to_owned());
            return;
        };
        let Some(fleet) = &self.live_fleet else {
            self.live_error = Some("Host connection worker unavailable.".to_owned());
            return;
        };
        let config = match self.create_config_payload(cx) {
            Ok(config) => config,
            Err(error) => {
                self.live_error = Some(error);
                return;
            }
        };
        self.create_request = self.create_request.wrapping_add(1);
        self.create_config = None;
        if let Err(error) = fleet.resolve_session_config(
            self.create_request,
            host.to_owned(),
            host_view.generation,
            provider,
            self.create_directory.read(cx).value().to_string(),
            config,
        ) {
            self.live_error = Some(error.to_owned());
        }
    }

    fn submit_create_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.create_pending {
            return;
        }
        let Some(NodeKey::Provider(host, _)) = self.drawer.clone() else {
            return;
        };
        let Some(provider) = self.create_provider.clone() else {
            self.live_error = Some("Choose an agent for the new session.".to_owned());
            return;
        };
        let Some(host_view) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
        else {
            self.live_error = Some("Host is not connected".to_owned());
            return;
        };
        let config = match self.create_config_payload(cx) {
            Ok(config) => config,
            Err(error) => {
                self.live_error = Some(error);
                return;
            }
        };
        let directory = self.create_directory.read(cx).value().to_string();
        if self.create_config.is_none() {
            self.live_error = Some("Wait for host session options to load.".to_owned());
            return;
        }
        if self.create_config_directory.as_deref() != Some(directory.as_str()) {
            self.live_error = Some("Refresh host options after changing the workspace.".to_owned());
            return;
        }
        if let Some(schema) = self.create_config.as_ref().map(|config| &config.schema) {
            for required in schema.required.iter().flatten() {
                let missing = config.get(required).is_none_or(|value| {
                    value.is_null()
                        || value.as_str().is_some_and(str::is_empty)
                        || value.as_array().is_some_and(|items| {
                            schema
                                .properties
                                .get(required)
                                .and_then(|property| property.min_items)
                                .is_some_and(|minimum| items.len() < minimum as usize)
                        })
                });
                if missing {
                    self.live_error = Some("Complete the host-required session options".to_owned());
                    return;
                }
            }
        }
        self.create_request = self.create_request.wrapping_add(1);
        let result = self
            .live_fleet
            .as_ref()
            .ok_or("Host connection worker unavailable.")
            .and_then(|fleet| {
                fleet.create_session(
                    self.create_request,
                    host.clone(),
                    host_view.generation,
                    provider,
                    directory,
                    config,
                    self.create_trust,
                )
            });
        match result {
            Ok(()) => {
                self.create_pending = true;
                self.live_error = None;
                self.create_submit_focus.focus(window);
            }
            Err(error) => self.live_error = Some(error.to_owned()),
        }
        cx.notify();
    }

    fn request_dispose_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_session_disposal.is_none() {
            self.dispose_confirm_open = true;
            self.dispose_confirm_focus.focus(window);
            cx.notify();
        }
    }

    fn cancel_dispose_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dispose_confirm_open = false;
        self.dispose_focus.focus(window);
        cx.notify();
    }

    fn dispose_live_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_session_disposal.is_some() {
            return;
        }
        let Some(NodeKey::Session(host, session)) = self.drawer.clone() else {
            return;
        };
        let Some(host_view) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
        else {
            self.live_error = Some("Host is not connected".to_owned());
            cx.notify();
            return;
        };
        let Some(fleet) = &self.live_fleet else {
            self.live_error = Some("Host connection worker unavailable.".to_owned());
            cx.notify();
            return;
        };
        self.dispose_request = self.dispose_request.wrapping_add(1);
        if let Err(error) = fleet.dispose_session(
            self.dispose_request,
            host.clone(),
            session.clone(),
            host_view.generation,
        ) {
            self.live_error = Some(error.to_owned());
            cx.notify();
            return;
        }
        self.dispose_confirm_open = false;
        self.pending_session_disposal = Some(PendingSessionDisposal {
            request: self.dispose_request,
            host,
            session,
            generation: host_view.generation,
            acknowledged: false,
        });
        self.dispose_focus.focus(window);
        cx.notify();
    }

    fn host_action(&mut self, id: &str, remove: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_host_action.is_some() {
            return;
        }
        let Some(host) = self.live_hosts.iter().find(|host| host.id == id) else {
            return;
        };
        if !remove && host.status != "connected" {
            self.close_drawer(window);
            self.connect_open = true;
            if self.connect_pending {
                self.connect_close_focus.focus(window);
            } else {
                self.connect_label
                    .update(cx, |input, cx| input.focus(window, cx));
            }
        } else {
            let generation = host.generation;
            let result = self
                .live_fleet
                .as_ref()
                .ok_or("Host connection worker unavailable.")
                .and_then(|fleet| {
                    if remove {
                        fleet.remove(id.to_owned())
                    } else {
                        fleet.reconnect(id.to_owned())
                    }
                });
            match result {
                Ok(()) => {
                    self.live_error = None;
                    self.pending_host_action = Some(PendingHostAction {
                        id: id.to_owned(),
                        action: if remove {
                            live::HostAction::Remove
                        } else {
                            live::HostAction::Reconnect
                        },
                        generation,
                        acknowledged: false,
                    });
                    self.drawer_close_focus.focus(window);
                }
                Err(error) => self.live_error = Some(error.to_owned()),
            }
        }
        cx.notify();
    }

    /// Native equivalent of the web client's "Reload Relay" recovery.
    /// The web page reload drops the in-memory fleet; here a fresh worker
    /// starts with no hosts, so stale shells referencing the dead worker
    /// (and its generations) are discarded the same way.
    fn restart_live_worker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.live_fleet.is_some() {
            return;
        }
        match live::LiveFleet::start() {
            Ok(fleet) => {
                self.live_fleet = Some(fleet);
                self.live_error = None;
                self.live_hosts.clear();
                self.pending_host_action = None;
                // The drawer target belonged to the dead worker; close it
                // so focus lands on the still-mounted connect control.
                self.close_drawer(window);
                if self.drawer.is_some() {
                    // close_drawer refused (stale disposal guard); move
                    // focus off the now-hidden restart control.
                    self.drawer_close_focus.focus(window);
                }
            }
            Err(_) => {
                self.live_error = Some("Host connection worker could not start.".to_owned());
            }
        }
        cx.notify();
    }

    fn start_topology_pan(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.topology_drag = None;
        if self.drawer.is_some() {
            return;
        }
        let offset = self.topology_scroll.offset();
        let origin = self.topology_scroll.bounds().origin;
        let position = event.position - origin - offset;
        if self
            .topology_graph(&self.search.read(cx).value())
            .nodes
            .iter()
            .any(|node| {
                position.x >= px(node.x)
                    && position.x < px(node.x + 128.0)
                    && position.y >= px(node.y)
                    && position.y < px(node.y + 32.0)
            })
        {
            return;
        }
        self.topology_drag = Some((event.position, offset));
    }

    fn pan_topology(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some((start, offset)) = self.topology_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.topology_drag = None;
            return;
        }
        let max = self.topology_scroll.max_offset();
        let delta = event.position - start;
        self.topology_scroll.set_offset(point(
            (offset.x + delta.x).clamp(-max.width, px(0.0)),
            (offset.y + delta.y).clamp(-max.height, px(0.0)),
        ));
        cx.notify();
    }

    fn cycle_live_session_focus(&self, window: &mut Window, backwards: bool) {
        let mut targets = vec![&self.drawer_close_focus];
        let tab_index = SESSION_TABS
            .iter()
            .position(|(tab, _)| *tab == self.session_tab)
            .unwrap_or(0);
        targets.push(&self.session_tab_focus[tab_index]);
        if self.session_tab != SessionTab::Info {
            targets.push(&self.snapshot_refresh_focus);
        }
        if let Some(Ok(snapshot)) = &self.session_snapshot {
            match self.session_tab {
                SessionTab::Conversation => {
                    if snapshot.visible_chats.len() > 1 {
                        targets.extend(
                            snapshot
                                .visible_chats
                                .iter()
                                .filter_map(|(uri, _)| self.chat_focus.get(uri)),
                        );
                    }
                    if let Some(NodeKey::Session(host_id, _)) = &self.drawer
                        && let Some(host) = self.live_hosts.iter().find(|host| &host.id == host_id)
                        && let Some(agent) = host
                            .agents
                            .iter()
                            .find(|agent| agent.provider == snapshot.provider)
                    {
                        if let Some(focus) = self.live_model_focus.get(&live_model_focus_key(
                            host_id,
                            &agent.provider,
                            DEFAULT_MODEL_ID,
                        )) {
                            targets.push(focus);
                        }
                        targets.extend(agent.models.iter().filter_map(|model| {
                            self.live_model_focus.get(&live_model_focus_key(
                                host_id,
                                &agent.provider,
                                &model.id,
                            ))
                        }));
                    }
                }
                SessionTab::Files => {
                    if snapshot.available_changesets.len() > 1 {
                        targets.extend(
                            snapshot
                                .available_changesets
                                .iter()
                                .filter_map(|(uri, _)| self.changeset_focus.get(uri)),
                        );
                    }
                    for (uri, template) in &snapshot.changeset_templates {
                        for name in live::changeset_parameter_names(template) {
                            if let Some(focus) = self
                                .changeset_param_focus
                                .get(&changeset_parameter_key(uri, name))
                            {
                                targets.push(focus);
                            }
                        }
                    }
                    if let Ok(Some((_, changes))) = &snapshot.changes {
                        targets.extend(
                            changes
                                .files
                                .iter()
                                .filter_map(|file| self.live_file_focus.get(&file.id)),
                        );
                        if snapshot.changeset_reviewable {
                            targets.extend(
                                changes
                                    .files
                                    .iter()
                                    .filter_map(|file| self.live_review_focus.get(&file.id)),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        let next = match targets.iter().position(|focus| focus.is_focused(window)) {
            Some(index) if backwards => (index + targets.len() - 1) % targets.len(),
            Some(index) => (index + 1) % targets.len(),
            None => 0,
        };
        targets[next].focus(window);
    }

    fn refresh_snapshot(
        &mut self,
        key: &NodeKey,
        chat: live::ChatSelection,
        changeset: live::ChangesetSelection,
    ) {
        self.pending_file_preview = None;
        self.session_snapshot = None;
        self.request_snapshot(key, chat, changeset);
    }

    /// Ask the worker for a fresh snapshot without clearing what is on screen,
    /// so a background re-read does not blank the drawer.
    fn request_snapshot(
        &mut self,
        key: &NodeKey,
        chat: live::ChatSelection,
        changeset: live::ChangesetSelection,
    ) {
        self.snapshot_request = self.snapshot_request.wrapping_add(1);
        if let Some(fleet) = &self.live_fleet {
            fleet.invalidate_snapshot();
        }
        if self.live
            && let NodeKey::Session(host, uri) = key
        {
            let result = self
                .live_hosts
                .iter()
                .find(|item| item.id == *host && item.status == "connected")
                .ok_or("Host is not connected")
                .and_then(|item| {
                    self.live_fleet
                        .as_ref()
                        .ok_or("Host connection worker unavailable.")
                        .and_then(|fleet| {
                            if self.session_tab == SessionTab::Conversation {
                                fleet.conversation_snapshot(
                                    self.snapshot_request,
                                    host.clone(),
                                    uri.clone(),
                                    item.generation,
                                    chat,
                                )
                            } else {
                                fleet.snapshot(
                                    self.snapshot_request,
                                    host.clone(),
                                    uri.clone(),
                                    item.generation,
                                    chat,
                                    changeset,
                                )
                            }
                        })
                });
            if let Err(error) = result {
                self.session_snapshot = Some(Err(error));
            }
        }
    }

    fn select_changeset_parameter(
        &mut self,
        template_uri: String,
        template: String,
        name: String,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Ok(snapshot)) = self.session_snapshot.as_ref() else {
            return;
        };
        let mut values = std::collections::HashMap::new();
        for parameter in live::changeset_parameter_names(&template) {
            let key = changeset_parameter_key(&template_uri, parameter);
            let value = if parameter == name {
                value.clone()
            } else {
                let Some(value) = self
                    .changeset_params
                    .get(&key)
                    .cloned()
                    .or_else(|| default_changeset_parameter(snapshot, parameter))
                else {
                    return;
                };
                value
            };
            values.insert(parameter.to_owned(), value);
        }
        let turns: Vec<_> = snapshot
            .changeset_turns
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        let Some(uri) = live::expand_changeset_with_values(&template, &turns, &values) else {
            return;
        };
        self.changeset_params
            .insert(changeset_parameter_key(&template_uri, &name), value);
        self.selected_changeset = Some(uri.clone());
        if let Some(key @ NodeKey::Session(_, _)) = self.drawer.clone() {
            self.snapshot_refresh_focus.focus(window);
            self.refresh_snapshot(
                &key,
                live::ChatSelection::Preferred(self.selected_chat.clone()),
                live::ChangesetSelection::Explicit(uri),
            );
            cx.notify();
        }
    }

    fn load_files_if_needed(&mut self) {
        if self.live
            && self.session_tab == SessionTab::Files
            && matches!(&self.session_snapshot, Some(Ok(snapshot)) if matches!(snapshot.changes, Err("Files were not loaded; refresh the Files tab.")))
            && let Some(key @ NodeKey::Session(_, _)) = self.drawer.clone()
        {
            self.refresh_snapshot(
                &key,
                live::ChatSelection::Preferred(self.selected_chat.clone()),
                live::ChangesetSelection::Preferred(self.selected_changeset.clone()),
            );
        }
    }

    fn inspect_live_file(&mut self, file: String) {
        let Some(NodeKey::Session(host, uri)) = self.drawer.clone() else {
            return;
        };
        let Some(changeset) = self.selected_changeset.clone() else {
            return;
        };
        let Some(generation) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
            .map(|item| item.generation)
        else {
            return;
        };
        self.snapshot_request = self.snapshot_request.wrapping_add(1);
        self.pending_file_preview = Some(file.clone());
        if let Some(Ok(snapshot)) = &mut self.session_snapshot {
            snapshot.preview = None;
        }
        let result = self
            .live_fleet
            .as_ref()
            .ok_or("Host connection worker unavailable.")
            .and_then(|fleet| {
                fleet.preview(
                    self.snapshot_request,
                    host,
                    uri,
                    generation,
                    live::ChatSelection::Preferred(self.selected_chat.clone()),
                    (changeset, file.clone()),
                )
            });
        if let Err(error) = result {
            self.pending_file_preview = None;
            if let Some(Ok(snapshot)) = &mut self.session_snapshot {
                snapshot.preview = Some((file, Err(error)));
            }
        }
    }

    fn review_live_file(
        &mut self,
        file: String,
        reviewed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_file_review.is_some() {
            return;
        }
        let Some(NodeKey::Session(host, session)) = self.drawer.clone() else {
            return;
        };
        let Some(changeset) = self.selected_changeset.clone() else {
            return;
        };
        let Some(snapshot) = self
            .session_snapshot
            .as_ref()
            .and_then(|result| result.as_ref().ok())
        else {
            return;
        };
        if !snapshot.changeset_reviewable
            || !snapshot.changes.as_ref().ok().is_some_and(|changes| {
                changes
                    .as_ref()
                    .is_some_and(|(_, changes)| changes.files.iter().any(|item| item.id == file))
            })
        {
            self.live_error = Some("Host does not allow reviewing this file".to_owned());
            cx.notify();
            return;
        }
        let Some(generation) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
            .map(|item| item.generation)
        else {
            return;
        };
        let request = live::FileReviewRequest {
            host: host.clone(),
            session: session.clone(),
            generation,
            changeset: changeset.clone(),
            file: file.clone(),
            reviewed,
        };
        let result = self
            .live_fleet
            .as_ref()
            .ok_or("Host connection worker unavailable.")
            .and_then(|fleet| fleet.review_file(request));
        if let Err(error) = result {
            self.live_error = Some(error.to_owned());
        } else {
            self.live_error = None;
            self.pending_file_review = Some(PendingFileReview {
                host,
                session,
                generation,
                changeset,
                file: file.clone(),
                reviewed,
            });
            if let Some(focus) = self.live_review_focus.get(&file) {
                focus.focus(window);
            }
        }
        cx.notify();
    }

    fn cancel_live_turn(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_turn_cancellation.is_some() {
            return;
        }
        let Some(NodeKey::Session(host, session)) = self.drawer.clone() else {
            return;
        };
        let Some(snapshot) = self
            .session_snapshot
            .as_ref()
            .and_then(|result| result.as_ref().ok())
        else {
            return;
        };
        let Some(chat) = snapshot
            .conversation
            .as_ref()
            .ok()
            .and_then(|chat| chat.as_ref())
        else {
            return;
        };
        let Some(turn_id) = chat.active_turn.as_ref().map(|turn| turn.id.clone()) else {
            return;
        };
        let Some(generation) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
            .map(|item| item.generation)
        else {
            self.live_error = Some("Host is not connected".to_owned());
            cx.notify();
            return;
        };
        let Some(fleet) = &self.live_fleet else {
            self.live_error = Some("Host connection worker unavailable.".to_owned());
            cx.notify();
            return;
        };
        if let Err(error) = fleet.cancel_turn(
            host.clone(),
            session.clone(),
            generation,
            chat.resource.to_string(),
            turn_id.clone(),
        ) {
            self.live_error = Some(error.to_owned());
        } else {
            self.pending_turn_cancellation = Some(PendingTurnCancellation {
                host,
                session,
                generation,
                chat: chat.resource.to_string(),
                turn_id,
            });
            self.cancel_turn_focus.focus(window);
        }
        cx.notify();
    }

    fn load_older_live_turns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_turn_page.is_some() {
            return;
        }
        let Some(NodeKey::Session(host, session)) = self.drawer.clone() else {
            return;
        };
        let Some(snapshot) = self
            .session_snapshot
            .as_ref()
            .and_then(|result| result.as_ref().ok())
        else {
            return;
        };
        let Some(chat) = snapshot
            .conversation
            .as_ref()
            .ok()
            .and_then(|chat| chat.as_ref())
        else {
            return;
        };
        let Some(cursor) = chat
            .turns_next_cursor
            .as_deref()
            .filter(|cursor| !cursor.is_empty())
            .map(str::to_owned)
        else {
            return;
        };
        let Some(generation) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
            .map(|item| item.generation)
        else {
            self.live_error = Some("Host is not connected".to_owned());
            cx.notify();
            return;
        };
        let chat_uri = chat.resource.to_string();
        let Some(fleet) = &self.live_fleet else {
            self.live_error = Some("Host connection worker unavailable.".to_owned());
            cx.notify();
            return;
        };
        if let Err(error) = fleet.load_older_turns(
            host.clone(),
            session.clone(),
            generation,
            chat_uri.clone(),
            cursor.clone(),
        ) {
            self.live_error = Some(error.to_owned());
        } else {
            self.live_error = None;
            self.pending_turn_page = Some(PendingTurnPage {
                host,
                session,
                generation,
                chat: chat_uri,
                cursor,
            });
            self.snapshot_refresh_focus.focus(window);
        }
        cx.notify();
    }

    fn queue_live_turn(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_turn_submission.is_some() {
            return;
        }
        let Some(NodeKey::Session(host, session)) = self.drawer.clone() else {
            return;
        };
        let text = self.live_prompt.read(cx).value().trim().to_owned();
        if text.is_empty() {
            self.live_error = Some("A prompt is required".to_owned());
            cx.notify();
            return;
        }
        let Some(snapshot) = self
            .session_snapshot
            .as_ref()
            .and_then(|result| result.as_ref().ok())
        else {
            return;
        };
        if snapshot.lifecycle == "failed" {
            self.live_error = Some("Session creation failed on the host".to_owned());
            cx.notify();
            return;
        }
        if snapshot.lifecycle == "creating" {
            self.live_error = Some("Session is still starting".to_owned());
            cx.notify();
            return;
        }
        let Some(chat) = snapshot
            .conversation
            .as_ref()
            .ok()
            .and_then(|chat| chat.as_ref())
        else {
            // Mirror the `chatPromptPolicy` queueTurn gate: a missing chat
            // throws the policy reason (`!chat` → "Chat is not ready").
            self.live_error = Some("Chat is not ready".to_owned());
            cx.notify();
            return;
        };
        let chat_uri = chat.resource.to_string();
        let Some(generation) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
            .map(|item| item.generation)
        else {
            self.live_error = Some("Host is not connected".to_owned());
            cx.notify();
            return;
        };
        let Some(fleet) = &self.live_fleet else {
            self.live_error = Some("Host connection worker unavailable.".to_owned());
            cx.notify();
            return;
        };
        match fleet.queue_turn(
            host.clone(),
            session.clone(),
            generation,
            chat_uri.clone(),
            text,
            self.live_model.clone(),
        ) {
            Ok(message_id) => {
                self.live_error = None;
                self.pending_turn_submission = Some(PendingTurnSubmission {
                    host,
                    session,
                    generation,
                    chat: chat_uri,
                    message_id,
                });
                self.queue_turn_focus.focus(window);
            }
            Err(error) => self.live_error = Some(error.to_owned()),
        }
        cx.notify();
    }

    fn confirm_live_tool(
        &mut self,
        approved: bool,
        selected_option_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_tool_confirmation.is_some() {
            return;
        }
        let Some(NodeKey::Session(host, session)) = self.drawer.clone() else {
            return;
        };
        let Some(snapshot) = self
            .session_snapshot
            .as_ref()
            .and_then(|result| result.as_ref().ok())
        else {
            return;
        };
        let Some(approval) = snapshot.approval.as_ref() else {
            return;
        };
        let Some(generation) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
            .map(|item| item.generation)
        else {
            self.live_error = Some("Host is not connected".to_owned());
            cx.notify();
            return;
        };
        let Some(fleet) = &self.live_fleet else {
            self.live_error = Some("Host connection worker unavailable.".to_owned());
            cx.notify();
            return;
        };
        let request = live::ToolConfirmationRequest {
            host: host.clone(),
            session: session.clone(),
            generation,
            chat: approval.chat.clone(),
            turn_id: approval.turn_id.clone(),
            tool_call_id: approval.tool_call_id.clone(),
            approved,
            result_confirmation: approval.result_confirmation,
            selected_option_id,
        };
        if let Err(error) = fleet.confirm_tool(request) {
            self.live_error = Some(error.to_owned());
        } else {
            self.pending_tool_confirmation = Some(PendingToolConfirmation {
                host,
                session,
                generation,
                chat: approval.chat.clone(),
                turn_id: approval.turn_id.clone(),
                tool_call_id: approval.tool_call_id.clone(),
            });
            self.approval_focus.focus(window);
        }
        cx.notify();
    }

    fn answer_live_input(
        &mut self,
        request_id: &str,
        response: ahp::ahp_types::state::ChatInputResponseKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_input_submission.is_some() {
            return;
        }
        let Some(NodeKey::Session(host, session)) = self.drawer.clone() else {
            return;
        };
        let Some(snapshot) = self
            .session_snapshot
            .as_ref()
            .and_then(|result| result.as_ref().ok())
        else {
            return;
        };
        let Some(request) = snapshot
            .input_requests
            .iter()
            .find(|request| request.request.id == request_id)
        else {
            return;
        };
        let Some(generation) = self
            .live_hosts
            .iter()
            .find(|item| item.id == host && item.status == "connected")
            .map(|item| item.generation)
        else {
            self.live_error = Some("Host is not connected".to_owned());
            cx.notify();
            return;
        };
        let answers = if matches!(
            response,
            ahp::ahp_types::state::ChatInputResponseKind::Accept
        ) {
            let mut answers = HashMap::new();
            // Mirror the web `(request.questions || [])`: a missing list
            // accepts with an empty answer map.
            let questions = request.request.questions.as_deref().unwrap_or(&[]);
            for question in questions {
                let is_integer = matches!(
                    question,
                    ahp::ahp_types::state::ChatInputQuestion::Integer(_)
                );
                let key = input_question_key(
                    &request.request.id,
                    match question {
                        ahp::ahp_types::state::ChatInputQuestion::Text(question) => &question.id,
                        ahp::ahp_types::state::ChatInputQuestion::Number(question)
                        | ahp::ahp_types::state::ChatInputQuestion::Integer(question) => {
                            &question.id
                        }
                        ahp::ahp_types::state::ChatInputQuestion::Boolean(question) => &question.id,
                        ahp::ahp_types::state::ChatInputQuestion::SingleSelect(question) => {
                            &question.id
                        }
                        ahp::ahp_types::state::ChatInputQuestion::MultiSelect(question) => {
                            &question.id
                        }
                        ahp::ahp_types::state::ChatInputQuestion::Unknown(value) => {
                            unknown_question_id(value)
                        }
                    },
                );
                let answer = match question {
                    ahp::ahp_types::state::ChatInputQuestion::Text(question) => {
                        let value = self
                            .input_inputs
                            .get(&key)
                            .map(|input| input.read(cx).value().trim().to_owned())
                            .unwrap_or_default();
                        if value.is_empty() {
                            if question.required.unwrap_or(false) {
                                self.live_error = Some(format!(
                                    "Complete {}",
                                    input_question_title(
                                        question.title.as_deref(),
                                        &question.message
                                    )
                                ));
                                cx.notify();
                                return;
                            }
                            ahp::ahp_types::state::ChatInputAnswer::Skipped(
                                ahp::ahp_types::state::ChatInputSkipped::default(),
                            )
                        } else {
                            let length = value.encode_utf16().count() as i64;
                            if question.min.is_some_and(|min| length < min)
                                || question.max.is_some_and(|max| length > max)
                            {
                                self.live_error = Some(format!(
                                    "Invalid answer for {}",
                                    input_question_title(
                                        question.title.as_deref(),
                                        &question.message
                                    )
                                ));
                                cx.notify();
                                return;
                            }
                            ahp::ahp_types::state::ChatInputAnswer::Submitted(
                                ahp::ahp_types::state::ChatInputAnswered {
                                    value: ahp::ahp_types::state::ChatInputAnswerValue::Text(
                                        ahp::ahp_types::state::ChatInputTextAnswerValue { value },
                                    ),
                                },
                            )
                        }
                    }
                    ahp::ahp_types::state::ChatInputQuestion::Number(question)
                    | ahp::ahp_types::state::ChatInputQuestion::Integer(question) => {
                        // The web keeps whitespace-only input through its
                        // non-empty filter (`Number("   ")` is 0), so
                        // emptiness here is the raw widget value, not the
                        // trimmed one.
                        let value = self
                            .input_inputs
                            .get(&key)
                            .map(|input| input.read(cx).value().to_string())
                            .unwrap_or_default();
                        if value.is_empty() && question.required.unwrap_or(false) {
                            self.live_error = Some(format!(
                                "Complete {}",
                                input_question_title(question.title.as_deref(), &question.message)
                            ));
                            cx.notify();
                            return;
                        }
                        match js_number(&value) {
                            value if value.is_finite() && (!is_integer || value.fract() == 0.0) => {
                                if question.min.is_some_and(|min| value < min)
                                    || question.max.is_some_and(|max| value > max)
                                {
                                    self.live_error = Some(format!(
                                        "Invalid answer for {}",
                                        input_question_title(
                                            question.title.as_deref(),
                                            &question.message
                                        )
                                    ));
                                    cx.notify();
                                    return;
                                }
                                ahp::ahp_types::state::ChatInputAnswer::Submitted(
                                    ahp::ahp_types::state::ChatInputAnswered {
                                        value: ahp::ahp_types::state::ChatInputAnswerValue::Number(
                                            ahp::ahp_types::state::ChatInputNumberAnswerValue {
                                                value,
                                            },
                                        ),
                                    },
                                )
                            }
                            _ if value.is_empty() => {
                                ahp::ahp_types::state::ChatInputAnswer::Skipped(
                                    ahp::ahp_types::state::ChatInputSkipped::default(),
                                )
                            }
                            _ => {
                                self.live_error =
                                    Some(format!("Invalid number for {}", question.id));
                                cx.notify();
                                return;
                            }
                        }
                    }
                    ahp::ahp_types::state::ChatInputQuestion::Boolean(question) => {
                        match self.input_booleans.get(&key).copied() {
                            // Mirror `readAnswer`: an unanswered boolean is
                            // skipped, or `Complete …` when required — the web
                            // select submits nothing until Yes/No is picked,
                            // so untouched never means `false`.
                            None => {
                                if question.required.unwrap_or(false) {
                                    self.live_error = Some(format!(
                                        "Complete {}",
                                        input_question_title(
                                            question.title.as_deref(),
                                            &question.message
                                        )
                                    ));
                                    cx.notify();
                                    return;
                                }
                                ahp::ahp_types::state::ChatInputAnswer::Skipped(
                                    ahp::ahp_types::state::ChatInputSkipped::default(),
                                )
                            }
                            Some(value) => ahp::ahp_types::state::ChatInputAnswer::Submitted(
                                ahp::ahp_types::state::ChatInputAnswered {
                                    value: ahp::ahp_types::state::ChatInputAnswerValue::Boolean(
                                        ahp::ahp_types::state::ChatInputBooleanAnswerValue {
                                            value,
                                        },
                                    ),
                                },
                            ),
                        }
                    }
                    ahp::ahp_types::state::ChatInputQuestion::SingleSelect(question) => {
                        let value = self
                            .input_selections
                            .get(&key)
                            .and_then(|values| values.first())
                            .cloned()
                            .unwrap_or_default();
                        let freeform = if question.allow_freeform_input.unwrap_or(false) {
                            self.input_freeform
                                .get(&key)
                                .map(|input| input.read(cx).value().trim().to_owned())
                                .filter(|value| !value.is_empty())
                                .map(|value| vec![value])
                        } else {
                            None
                        };
                        if value.is_empty() && freeform.is_none() {
                            if question.required.unwrap_or(false) {
                                self.live_error = Some(format!(
                                    "Complete {}",
                                    input_question_title(
                                        question.title.as_deref(),
                                        &question.message
                                    )
                                ));
                                cx.notify();
                                return;
                            }
                            ahp::ahp_types::state::ChatInputAnswer::Skipped(
                                ahp::ahp_types::state::ChatInputSkipped::default(),
                            )
                        } else if value.is_empty() && freeform.is_some()
                            || question.options.iter().any(|option| option.id == value)
                        {
                            ahp::ahp_types::state::ChatInputAnswer::Submitted(
                                ahp::ahp_types::state::ChatInputAnswered {
                                    value: ahp::ahp_types::state::ChatInputAnswerValue::Selected(
                                        ahp::ahp_types::state::ChatInputSelectedAnswerValue {
                                            value,
                                            freeform_values: freeform,
                                        },
                                    ),
                                },
                            )
                        } else {
                            self.live_error = Some(format!(
                                "Invalid answer for {}",
                                input_question_title(question.title.as_deref(), &question.message)
                            ));
                            cx.notify();
                            return;
                        }
                    }
                    ahp::ahp_types::state::ChatInputQuestion::MultiSelect(question) => {
                        let values = self.input_selections.get(&key).cloned().unwrap_or_default();
                        if values
                            .iter()
                            .any(|value| !question.options.iter().any(|option| option.id == *value))
                        {
                            self.live_error = Some(format!(
                                "Invalid answer for {}",
                                input_question_title(question.title.as_deref(), &question.message)
                            ));
                            cx.notify();
                            return;
                        }
                        let freeform = if question.allow_freeform_input.unwrap_or(false) {
                            self.input_freeform
                                .get(&key)
                                .map(|input| input.read(cx).value().trim().to_owned())
                                .filter(|value| !value.is_empty())
                                .map(|value| vec![value])
                        } else {
                            None
                        };
                        let selected_count =
                            values.len() as i64 + freeform.as_ref().map_or(0, Vec::len) as i64;
                        if question.min.is_some_and(|min| selected_count < min)
                            || question.max.is_some_and(|max| selected_count > max)
                        {
                            self.live_error = Some(format!(
                                "Invalid answer for {}",
                                input_question_title(question.title.as_deref(), &question.message)
                            ));
                            cx.notify();
                            return;
                        }
                        if values.is_empty() && freeform.is_none() {
                            if question.required.unwrap_or(false) {
                                self.live_error = Some(format!(
                                    "Complete {}",
                                    input_question_title(
                                        question.title.as_deref(),
                                        &question.message
                                    )
                                ));
                                cx.notify();
                                return;
                            }
                            ahp::ahp_types::state::ChatInputAnswer::Skipped(
                                ahp::ahp_types::state::ChatInputSkipped::default(),
                            )
                        } else {
                            ahp::ahp_types::state::ChatInputAnswer::Submitted(
                                ahp::ahp_types::state::ChatInputAnswered {
                                    value: ahp::ahp_types::state::ChatInputAnswerValue::SelectedMany(
                                        ahp::ahp_types::state::ChatInputSelectedManyAnswerValue {
                                            value: values,
                                            freeform_values: freeform,
                                        },
                                    ),
                                },
                            )
                        }
                    }
                    ahp::ahp_types::state::ChatInputQuestion::Unknown(raw) => {
                        // The web reads unknown kinds as text, then
                        // `validateInputAnswers` rejects a submitted value
                        // with the source-verbatim `Unsupported input question
                        // kind:` error; empty stays skipped (or `Complete …`
                        // when required), mirroring text.
                        let kind = unknown_question_kind(raw);
                        // `q.title || q.message` interpolates JS `undefined`
                        // when both fields are absent; an explicit empty
                        // string still falls through to empty.
                        let title =
                            match (unknown_question_title(raw), unknown_question_message(raw)) {
                                (None, "") if raw.get("message").is_none() => {
                                    "undefined".to_owned()
                                }
                                (title, message) => input_question_title(title, message).to_owned(),
                            };
                        let text = self
                            .input_inputs
                            .get(&key)
                            .map(|input| input.read(cx).value().trim().to_owned())
                            .unwrap_or_default();
                        if text.is_empty() {
                            if unknown_question_required(raw) {
                                self.live_error = Some(format!("Complete {title}"));
                                cx.notify();
                                return;
                            }
                            ahp::ahp_types::state::ChatInputAnswer::Skipped(
                                ahp::ahp_types::state::ChatInputSkipped::default(),
                            )
                        } else {
                            self.live_error =
                                Some(format!("Unsupported input question kind: {kind}"));
                            cx.notify();
                            return;
                        }
                    }
                };
                let question_id = match question {
                    ahp::ahp_types::state::ChatInputQuestion::Text(question) => question.id.clone(),
                    ahp::ahp_types::state::ChatInputQuestion::Number(question)
                    | ahp::ahp_types::state::ChatInputQuestion::Integer(question) => {
                        question.id.clone()
                    }
                    ahp::ahp_types::state::ChatInputQuestion::Boolean(question) => {
                        question.id.clone()
                    }
                    ahp::ahp_types::state::ChatInputQuestion::SingleSelect(question) => {
                        question.id.clone()
                    }
                    ahp::ahp_types::state::ChatInputQuestion::MultiSelect(question) => {
                        question.id.clone()
                    }
                    ahp::ahp_types::state::ChatInputQuestion::Unknown(value) => {
                        unknown_question_id(value).to_owned()
                    }
                };
                answers.insert(question_id, answer);
            }
            Some(answers)
        } else {
            None
        };
        let Some(fleet) = &self.live_fleet else {
            self.live_error = Some("Host connection worker unavailable.".to_owned());
            cx.notify();
            return;
        };
        let request = live::InputAnswerRequest {
            host: host.clone(),
            session: session.clone(),
            generation,
            entry_id: request.id.clone(),
            chat: request.chat.clone(),
            request_id: request.request.id.clone(),
            response,
            answers,
        };
        let request_id = request.request_id.clone();
        if let Err(error) = fleet.answer_input(request) {
            self.live_error = Some(error.to_owned());
        } else {
            self.live_error = None;
            self.pending_input_submission = Some(request_id);
            self.input_focus
                .values()
                .find(|focus| focus.is_focused(window))
                .cloned()
                .unwrap_or_else(|| self.queue_turn_focus.clone())
                .focus(window);
        }
        cx.notify();
    }

    fn open_drawer(&mut self, key: NodeKey, return_focus: FocusHandle, window: &mut Window) {
        if self.pending_session_disposal.is_some() {
            return;
        }
        self.create_open = false;
        self.create_pending = false;
        self.create_config = None;
        self.create_config_directory = None;
        self.create_config_inputs.clear();
        self.create_config_enums.clear();
        self.create_config_bools.clear();
        self.create_config_option_focus.clear();
        self.dispose_confirm_open = false;
        self.pending_turn_cancellation = None;
        self.pending_tool_confirmation = None;
        self.pending_input_submission = None;
        self.input_focus.clear();
        self.input_option_focus.clear();
        self.input_inputs.clear();
        self.input_freeform.clear();
        self.input_selections.clear();
        self.input_booleans.clear();
        self.live_model = None;
        self.selected_chat = None;
        self.chat_focus.clear();
        self.selected_changeset = None;
        self.changeset_focus.clear();
        self.session_tab = SessionTab::Conversation;
        self.refresh_snapshot(
            &key,
            live::ChatSelection::Preferred(None),
            live::ChangesetSelection::Preferred(None),
        );
        self.active_host = Some(match &key {
            NodeKey::Host(host) | NodeKey::Provider(host, _) | NodeKey::Session(host, _) => {
                host.clone()
            }
        });
        self.drawer = Some(key);
        self.drawer_return_focus = Some(return_focus);
        self.approval_expanded = false;
        self.session_tab = SessionTab::Conversation;
        self.file_preview = false;
        self.topology_drag = None;
        self.drawer_close_focus.focus(window);
    }

    fn close_drawer(&mut self, window: &mut Window) {
        if self.pending_session_disposal.is_some() {
            return;
        }
        self.create_open = false;
        self.create_pending = false;
        self.create_config = None;
        self.create_config_directory = None;
        self.create_config_inputs.clear();
        self.create_config_enums.clear();
        self.create_config_bools.clear();
        self.create_config_option_focus.clear();
        self.dispose_confirm_open = false;
        self.snapshot_request = self.snapshot_request.wrapping_add(1);
        self.pending_file_preview = None;
        if let Some(fleet) = &self.live_fleet {
            fleet.invalidate_snapshot();
        }
        self.session_snapshot = None;
        self.pending_turn_cancellation = None;
        self.pending_turn_submission = None;
        self.pending_turn_page = None;
        self.pending_file_review = None;
        self.pending_tool_confirmation = None;
        self.pending_input_submission = None;
        self.input_focus.clear();
        self.input_option_focus.clear();
        self.input_inputs.clear();
        self.input_freeform.clear();
        self.input_selections.clear();
        self.input_booleans.clear();
        self.live_model = None;
        self.selected_chat = None;
        self.chat_focus.clear();
        self.selected_changeset = None;
        self.changeset_focus.clear();
        self.changeset_param_focus.clear();
        self.changeset_params.clear();
        self.live_file_focus.clear();
        self.live_review_focus.clear();
        let return_to_catalog =
            self.drawer_return_focus.as_ref() == Some(&self.sessions_toggle_focus);
        let host_missing = self.live
            && matches!(&self.drawer, Some(NodeKey::Host(id) | NodeKey::Provider(id, _) | NodeKey::Session(id, _))
            if !self.live_hosts.iter().any(|host| &host.id == id));
        let session_missing = self.live
            && matches!(&self.drawer, Some(NodeKey::Session(id, uri))
                if !self.live_hosts.iter().any(|host| &host.id == id && host.sessions.iter().any(|session| &session.uri == uri)));
        self.drawer = None;
        if return_to_catalog {
            self.drawer_return_focus = None;
            self.sessions_toggle_focus.focus(window);
        } else if host_missing {
            self.drawer_return_focus = None;
            self.connect_button_focus.focus(window);
        } else if session_missing {
            self.drawer_return_focus = None;
            self.topology_focus.focus(window);
        } else if let Some(handle) = self.drawer_return_focus.take() {
            handle.focus(window);
        }
    }

    fn open_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcuts_return_focus = window.focused(cx);
        self.shortcuts_open = true;
        self.topology_drag = None;
        self.shortcuts_close_focus.focus(window);
    }

    fn close_shortcuts(&mut self, window: &mut Window) {
        self.shortcuts_open = false;
        if let Some(handle) = self.shortcuts_return_focus.take() {
            handle.focus(window);
        }
    }

    fn open_status(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.status_return_focus = window.focused(cx);
        self.status_open = true;
        self.topology_drag = None;
        self.status_close_focus.focus(window);
    }

    fn close_status(&mut self, window: &mut Window) {
        self.status_open = false;
        if let Some(handle) = self.status_return_focus.take() {
            handle.focus(window);
        }
    }

    /// Re-render the light field only when something it depends on changed.
    fn refresh_lightfield(&mut self, graph: &Graph, colors: theme::Colors, window: &mut Window) {
        if graph.nodes.is_empty() {
            if let Some(previous) = self.lightfield.take() {
                let _ = window.drop_image(previous.image);
            }
            return;
        }
        let width = graph.width.max(680.0);
        let height = graph.height.max(360.0);
        let nodes = lightfield_nodes(graph);
        let edges = lightfield_edges(graph);
        let mut geometry = lightfield::geometry(
            &nodes,
            &edges,
            lightfield_selection(graph, self.selected.as_ref()),
            0.0,
            height,
        );
        geometry.pulse = lightfield_pulse(graph, self.pulse.as_ref());
        let key = geometry.fingerprint()
            ^ u64::from(width.to_bits()).rotate_left(17)
            ^ u64::from(height.to_bits()).rotate_left(33);
        if self
            .lightfield
            .as_ref()
            .is_some_and(|cache| cache.key == key)
        {
            return;
        }
        let pixels = lightfield::render(
            width as u32,
            height as u32,
            0.0,
            &geometry,
            &lightfield::Palette::pugzui(colors.accent),
        );
        let image = lightfield_image(pixels, width as u32, height as u32);
        let previous = self.lightfield.replace(LightfieldCache {
            key,
            image,
            width,
            height,
        });
        // Every RenderImage gets its own atlas entry, so the one it replaces has
        // to be released rather than merely dropped.
        if let Some(previous) = previous {
            let _ = window.drop_image(previous.image);
        }
    }

    /// The pulse is bounded: it animates for its window and then stops.
    fn start_lightfield_pulse(
        &mut self,
        host: String,
        session: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pulse = Some(PulseSignal {
            host,
            session,
            started: Instant::now(),
        });
        cx.spawn_in(window, async move |this, cx| {
            loop {
                Timer::after(LIGHTFIELD_PULSE_FRAME).await;
                let running = cx.update(|_, cx| {
                    this.update(cx, |this, cx| {
                        let running = this
                            .pulse
                            .as_ref()
                            .is_some_and(|pulse| pulse.started.elapsed() < LIGHTFIELD_PULSE);
                        if !running {
                            this.pulse = None;
                        }
                        cx.notify();
                        running
                    })
                });
                if !matches!(running, Ok(Ok(true))) {
                    break;
                }
            }
        })
        .detach();
    }

    fn topology_graph(&self, query: &str) -> Graph {
        if self.live {
            live::graph(&self.live_hosts, query)
        } else {
            example::graph_for_query(query)
        }
    }

    fn topology_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.drawer.is_some() {
            return;
        }
        if self
            .facet_focus
            .iter()
            .any(|focus| focus.is_focused(window))
            && matches!(event.keystroke.key.as_str(), "space" | "enter")
        {
            return;
        }
        let graph = self.topology_graph(&self.search.read(cx).value());
        let Some(current) =
            topology_focused_node(&graph, self.focused.as_ref(), self.selected.as_ref())
        else {
            return;
        };
        let key = event.keystroke.key.as_str();
        if key == "space" {
            self.selected = Some(current.key());
            self.live_preview = self.live.then(|| current.clone());
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let next = match key {
            "home" => Some(0),
            "end" => graph.nodes.len().checked_sub(1),
            "left" | "right" | "up" | "down" => {
                let horizontal = matches!(key, "left" | "right");
                let direction = if matches!(key, "left" | "up") {
                    -1.0
                } else {
                    1.0
                };
                graph
                    .nodes
                    .iter()
                    .enumerate()
                    .filter_map(|(index, node)| {
                        let dx = node.x - current.x;
                        let dy = node.y - current.y;
                        let (primary, secondary) = if horizontal { (dx, dy) } else { (dy, dx) };
                        (primary * direction > 0.0)
                            .then_some((index, primary.abs() + secondary.abs() * 2.0))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(index, _)| index)
            }
            _ => return,
        };
        cx.stop_propagation();
        if let Some(index) = next {
            let node = &graph.nodes[index];
            self.focused = Some(node.key());
            self.topology_focus.focus(window);
            let mut offset = self.topology_scroll.offset();
            let bounds = self.topology_scroll.bounds();
            let left = px(node.x);
            let top = px(node.y);
            let right = left + px(128.0);
            let bottom = top + px(32.0);
            if left + offset.x < px(0.0) {
                offset.x = -left;
            } else if right + offset.x > bounds.size.width {
                offset.x = bounds.size.width - right;
            }
            if top + offset.y < px(0.0) {
                offset.y = -top;
            } else if bottom + offset.y > bounds.size.height {
                offset.y = bounds.size.height - bottom;
            }
            self.topology_scroll.set_offset(point(offset.x, offset.y));
            cx.notify();
        }
    }

    fn topology_key_up(&mut self, event: &KeyUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let graph = self.topology_graph(&self.search.read(cx).value());
        let current = topology_focused_node(&graph, self.focused.as_ref(), self.selected.as_ref());
        if self.drawer.is_none()
            && !self
                .facet_focus
                .iter()
                .any(|focus| focus.is_focused(window))
            && event.keystroke.key == "enter"
            && !event.keystroke.modifiers.modified()
            && let Some(node) = current
        {
            self.selected = Some(node.key());
            self.live_preview = self.live.then(|| node.clone());
            self.open_drawer(node.key(), self.topology_focus.clone(), window);
            cx.stop_propagation();
            cx.notify();
        }
    }
}

fn topology_focused_node<'a>(
    graph: &'a Graph,
    focused: Option<&NodeKey>,
    selected: Option<&NodeKey>,
) -> Option<&'a Node> {
    focused
        .and_then(|key| graph.node(key))
        .or_else(|| selected.and_then(|key| graph.node(key)))
        .or_else(|| graph.nodes.first())
}

fn path_tail(value: &str, fallback: &str) -> String {
    value
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn facet(node: &Node, index: usize) -> (&'static str, String) {
    match (node.kind, index) {
        (_, 0) => ("STATE", node.subtitle.to_string()),
        (Kind::Host, 1) => ("AGENTS", node.agent_count.to_string()),
        (Kind::Host, _) | (Kind::Provider, 1) => ("SESSIONS", node.session_count.to_string()),
        (Kind::Provider, _) => ("PROVIDER", node.provider.to_string()),
        (Kind::Session, 1) => ("WORKSPACE", path_tail(&node.workspace, "Not reported")),
        (Kind::Session, _) => (
            "AGENT",
            if node.provider.is_empty() {
                "Unknown"
            } else {
                &node.provider
            }
            .to_owned(),
        ),
    }
}

fn input_question_title<'a>(title: Option<&'a str>, message: &'a str) -> &'a str {
    // Mirror the `title || message` fallback used by the web form and the
    // native question labels: an empty title falls through to the message.
    title.filter(|title| !title.is_empty()).unwrap_or(message)
}

fn clip_text(value: &str, length: usize) -> String {
    if value.chars().count() <= length {
        value.to_owned()
    } else {
        value
            .chars()
            .take(length.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}

fn line(x: f32, y: f32, width: f32, height: f32, color: u32) -> impl IntoElement {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(width.max(1.0)))
        .h(px(height.max(1.0)))
        .bg(rgba(color))
}

fn relay_mark_segment(
    colors: theme::Colors,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> AnyElement {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(width))
        .h(px(height))
        .bg(rgb(colors.accent))
        .into_any_element()
}

fn relay_mark(colors: theme::Colors) -> impl IntoElement {
    div().relative().size(px(26.0)).children([
        relay_mark_segment(colors, 2.0, 3.0, 15.0, 2.0),
        relay_mark_segment(colors, 16.0, 3.0, 2.0, 9.0),
        relay_mark_segment(colors, 8.0, 9.0, 10.0, 2.0),
        relay_mark_segment(colors, 7.0, 9.0, 2.0, 15.0),
        relay_mark_segment(colors, 8.0, 22.0, 16.0, 2.0),
        relay_mark_segment(colors, 17.0, 3.0, 8.0, 2.0),
        relay_mark_segment(colors, 8.0, 15.0, 11.0, 2.0),
    ])
}

/// The lattice indices a viewport of `extent` shows when the map is scrolled
/// `panned` pixels past its origin: the first index at or before the viewport's
/// leading edge through the last one at or beyond its trailing edge, so no band
/// is left unpainted between whole cells.
fn visible_lattice(panned: f32, extent: f32, period: f32) -> std::ops::RangeInclusive<i32> {
    let first = (panned / period).floor() as i32;
    let last = ((panned + extent) / period).ceil() as i32;
    first..=last
}

/// The map's reference grid: a dot at each intersection, on the same 20px and
/// 80px periods the light field's reactive grid uses, so the two stay aligned.
/// It is painted rather than laid out, because a dot per intersection is
/// thousands of marks; it covers the scroll viewport rather than the graph, so
/// the field never ends at the graph's edge.
fn topology_grid(colors: theme::Colors, state: Entity<Relay>) -> impl IntoElement {
    const MINOR: f32 = lightfield::GRID_MINOR;
    /// Minor cells between major dots.
    const MAJOR_EVERY: i32 = (lightfield::GRID_MAJOR / lightfield::GRID_MINOR) as i32;
    const DOT: f32 = 2.0;
    const MAJOR_DOT: f32 = 3.0;
    let minor = (colors.separator << 8) | 0x73;
    let major = (colors.muted << 8) | 0x80;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            let viewport = state.read(cx).topology_scroll.bounds();
            let width = f32::from(viewport.size.width);
            let height = f32::from(viewport.size.height);
            if width < 1.0 || height < 1.0 {
                return;
            }
            // The canvas is the map's own origin and the scroll offset has
            // already moved it; `paint_quad` adds no translation of its own.
            let left = f32::from(bounds.origin.x);
            let top = f32::from(bounds.origin.y);
            let panned_x = f32::from(viewport.origin.x) - left;
            let panned_y = f32::from(viewport.origin.y) - top;
            for column in visible_lattice(panned_x, width, MINOR) {
                let x = left + column as f32 * MINOR;
                for row in visible_lattice(panned_y, height, MINOR) {
                    let y = top + row as f32 * MINOR;
                    let is_major = column % MAJOR_EVERY == 0 && row % MAJOR_EVERY == 0;
                    let dot = if is_major { MAJOR_DOT } else { DOT };
                    let color = if is_major { major } else { minor };
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(px(x - dot / 2.0), px(y - dot / 2.0)),
                                size(px(dot), px(dot)),
                            ),
                            rgba(color),
                        )
                        .corner_radii(px(dot / 2.0)),
                    );
                }
            }
        },
    )
    .absolute()
    .left(px(0.0))
    .top(px(0.0))
    .w_full()
    .h_full()
}

fn topology_empty(colors: theme::Colors, title: &str, hint: &str) -> impl IntoElement {
    div()
        .w_full()
        .min_h(px(320.0))
        .p(px(32.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(12.0))
        .child(
            div()
                .size(px(48.0))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(rgb(colors.separator))
                .text_size(px(20.0))
                .text_color(rgb(colors.accent))
                .child("+"),
        )
        .child(
            div()
                .text_size(px(16.0))
                .font_medium()
                .text_center()
                .child(title.to_owned()),
        )
        .child(
            div()
                .w_full()
                .max_w(px(352.0))
                .text_center()
                .text_size(px(13.0))
                .text_color(rgb(colors.muted))
                .child(hint.to_owned()),
        )
}

fn edge_lines(colors: theme::Colors, edge: &example::Edge) -> Vec<gpui::AnyElement> {
    let points = [
        (edge.from_x, edge.from_y),
        (edge.from_x, edge.bend_y),
        (edge.to_x, edge.bend_y),
        (edge.to_x, edge.to_y),
    ];
    let color = if edge.stale {
        (colors.muted << 8) | 0x59
    } else {
        (colors.secondary << 8) | 0x73
    };
    let mut lines = Vec::new();
    let mut distance = 0.0_f32;
    for segment in points.windows(2) {
        let (x, y) = segment[0];
        let (end_x, end_y) = segment[1];
        let dx = end_x - x;
        let dy = end_y - y;
        let length = dx.abs() + dy.abs();
        if length == 0.0 {
            continue;
        }
        let mut cursor = 0.0_f32;
        while cursor < length {
            let phase = (distance + cursor) % 7.0;
            let visible = !edge.stale || phase < 3.0;
            let step = if !edge.stale {
                length - cursor
            } else if visible {
                (3.0 - phase).min(length - cursor)
            } else {
                (7.0 - phase).min(length - cursor)
            };
            if visible {
                let start_x = x + dx / length * cursor;
                let start_y = y + dy / length * cursor;
                let finish_x = x + dx / length * (cursor + step);
                let finish_y = y + dy / length * (cursor + step);
                lines.push(
                    line(
                        start_x.min(finish_x),
                        start_y.min(finish_y),
                        (finish_x - start_x).abs(),
                        (finish_y - start_y).abs(),
                        color,
                    )
                    .into_any_element(),
                );
            }
            cursor += step;
        }
        distance += length;
    }
    lines
}

fn lightfield_nodes(graph: &Graph) -> Vec<lightfield::NodeRef<'_>> {
    graph
        .nodes
        .iter()
        .map(|node| lightfield::NodeRef {
            kind: match node.kind {
                Kind::Host => lightfield::Kind::Host,
                Kind::Provider => lightfield::Kind::Provider,
                Kind::Session => lightfield::Kind::Session,
            },
            x: node.x,
            y: node.y,
            status: node.status.as_str(),
            host_id: node.host_id.as_str(),
            uri: node.uri.as_deref(),
            provider: node.provider.as_str(),
        })
        .collect()
}

fn lightfield_edges(graph: &Graph) -> Vec<lightfield::EdgeRef<'_>> {
    graph
        .edges
        .iter()
        .map(|edge| lightfield::EdgeRef {
            x1: edge.from_x,
            y1: edge.from_y,
            bend_y: edge.bend_y,
            x2: edge.to_x,
            y2: edge.to_y,
            stale: edge.stale,
            host_id: edge.host_id.as_str(),
            session_uri: edge.session_uri.as_deref(),
        })
        .collect()
}

/// The browser packs a session selection as host plus URI, a provider selection
/// as host plus provider, and a host selection as the host alone.
fn lightfield_selection<'a>(
    graph: &'a Graph,
    selected: Option<&'a NodeKey>,
) -> Option<lightfield::Selection<'a>> {
    let key = selected?;
    let node = graph.nodes.iter().find(|node| &node.key() == key)?;
    Some(lightfield::Selection {
        host_id: node.host_id.as_str(),
        uri: match key {
            NodeKey::Session(_, uri) => Some(uri.as_str()),
            _ => None,
        },
        provider: match key {
            NodeKey::Provider(_, provider) => Some(provider.as_str()),
            _ => None,
        },
    })
}

fn lightfield_pulse(graph: &Graph, signal: Option<&PulseSignal>) -> Option<lightfield::Pulse> {
    let signal = signal?;
    let elapsed = signal.started.elapsed();
    if elapsed >= LIGHTFIELD_PULSE {
        return None;
    }
    let edge = graph.edges.iter().find(|edge| {
        edge.host_id == signal.host && edge.session_uri.as_deref() == Some(signal.session.as_str())
    })?;
    Some(lightfield::Pulse {
        x1: edge.from_x,
        y1: edge.from_y,
        x2: edge.to_x,
        y2: edge.to_y,
        time: elapsed.as_secs_f32(),
    })
}

fn lightfield_image(pixels: Vec<u8>, width: u32, height: u32) -> Arc<RenderImage> {
    let buffer = image::ImageBuffer::from_raw(width, height, pixels)
        .expect("light field buffer matches its dimensions");
    Arc::new(RenderImage::new([image::Frame::new(buffer)]))
}

/// The browser pulses only when the host confirms the turn ended. Positive
/// evidence is required: both snapshots must have loaded the same chat, the turn
/// that was active must be gone, and it must now appear in the completed list.
/// A subscription failure, a chat switch, a re-seed that drops the turn, or a
/// chat that was merely idle never pulses. A cancellation the host does not
/// record as a completed turn is not pulsed either, because state cannot tell it
/// apart from a turn that simply went away.
fn pulse_after_turn(
    chat_before: Option<&str>,
    active_before: Option<&str>,
    chat_after: Option<&str>,
    active_after: Option<&str>,
    completed_after: &[&str],
) -> bool {
    chat_before.is_some()
        && chat_before == chat_after
        && active_after.is_none()
        && active_before.is_some_and(|turn| completed_after.contains(&turn))
}

fn loaded_chat(snapshot: &live::SessionSnapshot) -> Option<&ahp::ahp_types::state::ChatState> {
    snapshot
        .conversation
        .as_ref()
        .ok()
        .and_then(|conversation| conversation.as_deref())
}

fn active_turn_id(snapshot: &live::SessionSnapshot) -> Option<&str> {
    loaded_chat(snapshot)
        .and_then(|conversation| conversation.active_turn.as_ref())
        .map(|turn| turn.id.as_str())
}

/// The previous snapshot is the baseline, and it must have loaded: a snapshot
/// that failed to read leaves nothing to compare against, so a turn that ends
/// across a failure, a disconnect or a reopened drawer never pulses.
fn pulse_baseline<'a>(
    stored: Option<&'a Result<live::SessionSnapshot, &'static str>>,
) -> Option<&'a live::SessionSnapshot> {
    stored.and_then(|previous| previous.as_ref().ok())
}

fn turn_just_completed(previous: &live::SessionSnapshot, next: &live::SessionSnapshot) -> bool {
    let completed: Vec<&str> = loaded_chat(next)
        .map(|conversation| {
            conversation
                .turns
                .iter()
                .map(|turn| turn.id.as_str())
                .collect()
        })
        .unwrap_or_default();
    pulse_after_turn(
        loaded_chat(previous).and(previous.selected_chat.as_deref()),
        active_turn_id(previous),
        loaded_chat(next).and(next.selected_chat.as_deref()),
        active_turn_id(next),
        &completed,
    )
}

/// What the host pushes about one session: the fields that move when a turn
/// starts, ends or is cancelled, and that are therefore worth re-reading the
/// conversation for.
type SessionSummary = (u32, Option<String>, String);

/// The collapsed host rail: the vertically stacked `H o s t s` and the host
/// count the design contract calls for, instead of one mark per host.
fn rail_stacked_label(colors: theme::Colors, count: usize) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .py(px(10.0))
        .gap(px(1.0))
        .text_color(rgb(colors.muted))
        .children(
            "Hosts"
                .chars()
                .map(|letter| div().child(letter.to_string())),
        )
        .child(
            div()
                .pt(px(8.0))
                .text_color(rgb(colors.secondary))
                .child(count.to_string()),
        )
}

fn session_summary(hosts: &[live::HostView], host_id: &str, uri: &str) -> Option<SessionSummary> {
    hosts
        .iter()
        .find(|host| host.id == host_id)?
        .sessions
        .iter()
        .find(|session| session.uri == uri)
        .map(|session| {
            (
                session.status,
                session.activity.clone(),
                session.modified_at.clone(),
            )
        })
}

fn topology(
    graph: &Graph,
    state: &Relay,
    window: &Window,
    cx: &mut Context<Relay>,
) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    let focused = topology_focused_node(graph, state.focused.as_ref(), state.selected.as_ref())
        .map(Node::key);
    div()
        .relative()
        .w(px(graph.width))
        .h(px(graph.height.max(360.0)))
        .flex_none()
        .children(state.lightfield.as_ref().map(|cache| {
            img(ImageSource::Render(cache.image.clone()))
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(px(cache.width))
                .h(px(cache.height))
                .object_fit(ObjectFit::Fill)
        }))
        .child(topology_grid(colors, cx.entity()))
        .children(graph.edges.iter().flat_map(|edge| edge_lines(colors, edge)))
        .children(graph.nodes.iter().map(|node| {
            let key = node.key();
            let node_snapshot = node.clone();
            let color = match node.status.as_str() {
                "working" => colors.accent,
                "attention" => colors.warning,
                "error" | "failed" => colors.danger,
                _ => colors.secondary,
            };
            let level = match node.status.as_str() {
                "working" | "attention" | "error" | "failed" => 4,
                "connecting" | "reconnecting" => 3,
                "connected" => 2,
                "idle" => 1,
                _ => 0,
            };
            let show_status = matches!(
                node.status.as_str(),
                "working"
                    | "attention"
                    | "error"
                    | "failed"
                    | "connecting"
                    | "reconnecting"
                    | "offline"
                    | "disconnected"
                    | "stale"
            );
            let current_facet = state
                .hovered_facet
                .as_ref()
                .filter(|(node_key, _)| *node_key == key)
                .map(|(_, facet_index)| *facet_index)
                .or_else(|| state.facets.get(&key).copied());
            div()
                .id(gpui::SharedString::from(format!("{key:?}")))
                .absolute()
                .left(px(node.x))
                .top(px(node.y))
                .w(px(128.0))
                .h(px(32.0))
                .flex()
                .bg(rgb(colors.surface))
                .border_1()
                .border_color(rgb(
                    if state.topology_focus.is_focused(window) && focused.as_ref() == Some(&key) {
                        colors.accent
                    } else if state.selected.as_ref() == Some(&key) {
                        colors.secondary
                    } else {
                        colors.separator
                    },
                ))
                .child(
                    div()
                        .w(px(16.0))
                        .h_full()
                        .flex()
                        .flex_col()
                        .border_r_1()
                        .border_color(rgb(colors.separator))
                        .children((0..3).map(|facet_index| {
                            let hover_key = key.clone();
                            let click_key = key.clone();
                            let keyboard_key = key.clone();
                            div()
                                .id(("facet", facet_index))
                                .when(focused.as_ref() == Some(&key), |facet| {
                                    facet.track_focus(
                                        &state.facet_focus[facet_index].clone().tab_stop(
                                            state.drawer.is_none()
                                                && !state.shortcuts_open
                                                && !state.status_open,
                                        ),
                                    )
                                })
                                .focus(|style| style.bg(rgb(colors.separator)))
                                .h(px(10.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(match facet_index {
                                    0 => div()
                                        .w(px(5.0))
                                        .h(px(5.0))
                                        .border_1()
                                        .border_color(rgb(colors.muted))
                                        .rounded_full(),
                                    1 => div()
                                        .w(px(5.0))
                                        .h(px(5.0))
                                        .border_1()
                                        .border_color(rgb(colors.muted)),
                                    _ => div().flex().flex_col().gap(px(1.0)).children((0..3).map(
                                        |_| div().w(px(5.0)).h(px(1.0)).bg(rgb(colors.muted)),
                                    )),
                                })
                                .on_hover(cx.listener(move |this, hovered, _, cx| {
                                    if *hovered {
                                        this.hovered_facet = Some((hover_key.clone(), facet_index));
                                    } else if this.hovered_facet.as_ref().is_some_and(
                                        |(node, index)| node == &hover_key && *index == facet_index,
                                    ) {
                                        this.hovered_facet = None;
                                    }
                                    cx.notify();
                                }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.facets.insert(click_key.clone(), facet_index);
                                    cx.stop_propagation();
                                    cx.notify();
                                }))
                                .on_key_down(cx.listener(
                                    move |this, event: &KeyDownEvent, _, cx| {
                                        if matches!(event.keystroke.key.as_str(), "space" | "enter")
                                        {
                                            this.facets.insert(keyboard_key.clone(), facet_index);
                                            cx.stop_propagation();
                                            cx.notify();
                                        }
                                    },
                                ))
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .justify_between()
                        .px(px(4.0))
                        .overflow_hidden()
                        .when_some(current_facet, |body, facet_index| {
                            let (label, value) = facet(node, facet_index);
                            body.child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(rgb(colors.muted))
                                    .child(label),
                            )
                            .child(div().text_size(px(10.0)).child(clip_text(&value, 16)))
                        })
                        .when(current_facet.is_none(), |body| {
                            body.child(div().text_size(px(10.0)).child(clip_text(
                                &node.title,
                                if node.kind == Kind::Session { 16 } else { 17 },
                            )))
                        })
                        .child(
                            div()
                                .flex()
                                .items_end()
                                .justify_between()
                                .pb(px(2.0))
                                .child(div().flex().gap(px(3.0)).pb(px(1.0)).children((0..4).map(
                                    |bar| {
                                        div().w(px(8.0)).h(px(2.0)).bg(rgb(if bar < level {
                                            color
                                        } else {
                                            colors.separator
                                        }))
                                    },
                                )))
                                .when(show_status && current_facet.is_none(), |row| {
                                    row.child(
                                        div()
                                            .text_size(px(9.0))
                                            .line_height(px(10.0))
                                            .text_color(rgb(color))
                                            .child(node.status.to_uppercase()),
                                    )
                                }),
                        ),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focused = Some(key.clone());
                    this.topology_focus.focus(window);
                    this.selected = Some(key.clone());
                    this.live_preview = this.live.then(|| node_snapshot.clone());
                    cx.notify();
                }))
        }))
}

fn status_dialog(state: &Relay, cx: &mut Context<Relay>) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    div()
        .id("status-backdrop")
        .absolute()
        .size_full()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgba((colors.bg << 8) | 0xb3))
        .on_click(cx.listener(|this, _, window, cx| {
            this.close_status(window);
            cx.notify();
        }))
        .child(
            div()
                .id("status-dialog")
                .occlude()
                .w(px(400.0))
                .max_w_full()
                .bg(rgb(colors.surface))
                .border_1()
                .border_color(rgb(colors.separator))
                .child(
                    div()
                        .h(px(56.0))
                        .px(px(16.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(rgb(colors.separator))
                        .child("STATUS")
                        .child(
                            div()
                                .id("close-status")
                                .track_focus(&state.status_close_focus)
                                .focus(|style| style.text_color(rgb(colors.accent)))
                                .child("×")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_status(window);
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .p(px(16.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .when_some(state.live_error.clone(), |body, error| {
                            body.child(div().text_color(rgb(colors.danger)).child(error))
                        })
                        .when(state.live_error.is_none(), |body| {
                            body.child(
                                div()
                                    .text_color(rgb(colors.muted))
                                    .child("No current error."),
                            )
                        })
                        .when(state.live_error.is_some(), |body| {
                            body.child(
                                div()
                                    .id("dismiss-status-error")
                                    .track_focus(&state.status_dismiss_focus)
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .child("Dismiss")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.live_error = None;
                                        cx.notify();
                                    })),
                            )
                        }),
                ),
        )
}

fn live_host_details(
    host: Option<&live::HostView>,
    state: &Relay,
    cx: &mut Context<Relay>,
) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    let status = host.map_or("unavailable", |host| host.status);
    div()
        .occlude()
        .absolute()
        .top(px(36.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .w(px(352.0))
        .max_w_full()
        .flex()
        .flex_col()
        .bg(rgb(colors.surface))
        .border_l_1()
        .border_color(rgb(colors.separator))
        .child(
            div()
                .h(px(56.0))
                .px(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(rgb(colors.separator))
                .child("HOST DETAILS")
                .child(
                    div()
                        .id("close-live-details")
                        .track_focus(&state.drawer_close_focus)
                        .focus(|style| style.text_color(rgb(colors.accent)))
                        .child("×")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_drawer(window);
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .id("live-details-content")
                .flex_1()
                .overflow_y_scroll()
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .when_some(host, |body, host| {
                    let stale = host.status != "connected";
                    let connected = !stale;
                    let id = host.id.clone();
                    let reconnect_id = id.clone();

                    let reconnect_focus = state.live_reconnect_focus[&id].clone();
                    let remove_focus = state.live_remove_focus[&id].clone();
                    body.child(div().text_size(px(16.0)).child(host.label.clone()))
                        .child(format!("Status: {status}"))
                        .child(format!(
                            "Protocol: {}{}",
                            match host.protocol_version.as_deref() {
                                Some(version) => format!("AHP {version}"),
                                None => "Pending".to_owned(),
                            },
                            if stale && host.protocol_version.is_some() {
                                " (stale)"
                            } else {
                                ""
                            }
                        ))
                        .child(format!(
                            "Agents (cached): {}{}",
                            host.agents.len(),
                            if stale { " (stale)" } else { "" }
                        ))
                        .child(format!(
                            "Sessions (cached, freshness unverified): {}",
                            host.sessions.len()
                        ))
                        .when_some(host.last_error.as_deref(), |body, error| {
                            body.child(div().text_color(rgb(colors.danger)).child(error.to_owned()))
                        })
                        .when_some(state.live_error.as_ref(), |body, error| {
                            body.child(div().text_color(rgb(colors.danger)).child(error.clone()))
                        })
                        .when(state.pending_host_action.is_some(), |body| {
                            body.child(
                                div()
                                    .text_color(rgb(colors.muted))
                                    .child("Waiting for host state…"),
                            )
                        })
                        .when(state.live_fleet.is_none(), |body| {
                            body.child(
                                div()
                                    .text_color(rgb(colors.muted))
                                    .child("Host connection worker unavailable."),
                            )
                            .child(
                                div()
                                    .id("restart-live-worker")
                                    .track_focus(
                                        &state
                                            .restart_worker_focus
                                            .clone()
                                            .tab_stop(state.drawer.is_some()),
                                    )
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .child("Restart worker")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.restart_live_worker(window, cx);
                                    })),
                            )
                        })
                        .when(
                            state.pending_host_action.is_none() && state.live_fleet.is_some(),
                            |body| {
                                body.child(
                                    div()
                                        .flex()
                                        .gap(px(16.0))
                                        .child(
                                            div()
                                                .id(&reconnect_focus)
                                                .track_focus(&reconnect_focus)
                                                .focus(|style| style.text_color(rgb(colors.accent)))
                                                .child(if connected {
                                                    "Reconnect"
                                                } else {
                                                    "Update credentials"
                                                })
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.host_action(
                                                            &reconnect_id,
                                                            false,
                                                            window,
                                                            cx,
                                                        )
                                                    },
                                                )),
                                        )
                                        .child(
                                            div()
                                                .id(&remove_focus)
                                                .track_focus(&remove_focus)
                                                .focus(|style| style.text_color(rgb(colors.accent)))
                                                .child("Remove host")
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.host_action(&id, true, window, cx)
                                                    },
                                                )),
                                        ),
                                )
                            },
                        )
                        .child("Advertised agents · cached; freshness unverified")
                        .children(host.agents.iter().map(|agent| {
                            div()
                                .text_color(rgb(colors.muted))
                                .child(format!("{} · {}", agent.display_name, agent.provider))
                        }))
                })
                .when(host.is_none(), |body| {
                    body.child("Host is no longer available.")
                }),
        )
}

fn live_provider_details(
    host: Option<&live::HostView>,
    provider: &str,
    state: &Relay,
    cx: &mut Context<Relay>,
) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    let provider = provider.to_owned();
    div()
        .occlude()
        .absolute()
        .top(px(36.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .w(px(352.0))
        .max_w_full()
        .flex()
        .flex_col()
        .bg(rgb(colors.surface))
        .border_l_1()
        .border_color(rgb(colors.separator))
        .child(
            div()
                .h(px(56.0))
                .px(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(rgb(colors.separator))
                .child("PROVIDER")
                .child(
                    div()
                        .id("close-live-provider")
                        .track_focus(&state.drawer_close_focus)
                        .focus(|style| style.text_color(rgb(colors.accent)))
                        .child("×")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_drawer(window);
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .when_some(host, |body, host| {
                    let title = host
                        .agents
                        .iter()
                        .find(|agent| agent.provider == provider)
                        .map(|agent| agent.display_name.as_str())
                        .filter(|title| !title.is_empty())
                        .unwrap_or(if provider.is_empty() {
                            "Unknown agent"
                        } else {
                            &provider
                        });
                    let advertised = host.agents.iter().any(|agent| agent.provider == provider);
                    // Mirror the web provider drawer: the action is only
                    // available on a connected host that still advertises the
                    // provider. The worker re-validates both before dispatch.
                    let can_create =
                        state.live_fleet.is_some() && host.status == "connected" && advertised;
                    let action_color = if can_create {
                        colors.accent
                    } else {
                        colors.muted
                    };
                    let count = host
                        .sessions
                        .iter()
                        .filter(|session| session.provider == provider)
                        .count();
                    body.child(div().text_size(px(16.0)).child(title.to_owned()))
                        .child(provider.clone())
                        .child(if advertised {
                            "Cached advertisement · freshness unverified"
                        } else {
                            "Not in cached host advertisements · freshness unverified"
                        })
                        .child(format!("{count} cached sessions · freshness unverified"))
                        .when(!state.create_open, |body| {
                            let host_id = host.id.clone();
                            let initial_provider = provider.clone();
                            body.child(
                                div()
                                    .id("new-live-session")
                                    .track_focus(&state.create_submit_focus.clone().tab_stop(
                                        can_create && !state.shortcuts_open && !state.status_open,
                                    ))
                                    .focus(|style| style.text_color(rgb(action_color)))
                                    .text_color(rgb(action_color))
                                    .child("+ New session")
                                    .when(can_create, |button| {
                                        button.on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_create_session(
                                                host_id.clone(),
                                                initial_provider.clone(),
                                                window,
                                                cx,
                                            );
                                        }))
                                    }),
                            )
                        })
                        .when(state.create_open, |body| {
                            body.child(live_create_session_details(host, &provider, state, cx))
                        })
                })
                .when(host.is_none(), |body| {
                    body.child("Host is no longer available.")
                }),
        )
}

fn live_create_session_details(
    host: &live::HostView,
    provider: &str,
    state: &Relay,
    cx: &mut Context<Relay>,
) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    let host_id = host.id.clone();
    let pending = state.create_pending;
    let selected_provider = state.create_provider.as_deref();
    let config = state.create_config.as_ref();
    div()
        .border_t_1()
        .border_color(rgb(colors.separator))
        .pt(px(12.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(div().text_size(px(16.0)).child("New session"))
        .child(div().text_color(rgb(colors.muted)).child("Agent"))
        .children(host.agents.iter().enumerate().map(|(index, agent)| {
            let agent_provider = agent.provider.clone();
            let selected = selected_provider == Some(agent.provider.as_str());
            let label = if agent.display_name.is_empty() {
                agent.provider.clone()
            } else {
                format!("{} · {}", agent.display_name, agent.provider)
            };
            let host_id = host_id.clone();
            let focus = state
                .create_agent_focus
                .get(&create_agent_focus_key(&host_id, &agent.provider))
                .unwrap_or(&state.create_submit_focus);
            let keyboard_host_id = host_id.clone();
            let keyboard_provider = agent_provider.clone();
            div()
                .id(("create-agent", index))
                .track_focus(focus)
                .focus(|style| style.text_color(rgb(colors.accent)))
                .text_color(rgb(if selected { colors.accent } else { colors.text }))
                .child(format!("{} {label}", if selected { "●" } else { "○" }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !pending {
                        this.open_create_session(
                            host_id.clone(),
                            agent_provider.clone(),
                            window,
                            cx,
                        );
                    }
                }))
                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        if !this.create_pending {
                            this.open_create_session(
                                keyboard_host_id.clone(),
                                keyboard_provider.clone(),
                                window,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }
                }))
        }))
        .child(div().text_color(rgb(colors.muted)).child("Workspace (file URI)"))
        .child(Input::new(&state.create_directory).disabled(pending))
        .child(
            div()
                .id("create-trust-workspace")
                .track_focus(&state.create_trust_focus)
                .focus(|style| style.text_color(rgb(colors.accent)))
                .text_color(rgb(if pending { colors.muted } else { colors.text }))
                .child(format!(
                    "{} Trust this workspace for this browser tab",
                    if state.create_trust { "☑" } else { "☐" }
                ))
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.create_pending {
                        this.create_trust = !this.create_trust;
                        cx.notify();
                    }
                }))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        if !this.create_pending {
                            this.create_trust = !this.create_trust;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }
                })),
        )
        .child(
            div()
                .text_color(rgb(colors.muted))
                .child("Allow this agent to load repository instructions and hooks. Only select projects you trust."),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child("Host session options")
                .child(
                    div()
                        .id("refresh-create-options")
                        .track_focus(&state.create_refresh_focus)
                        .focus(|style| style.text_color(rgb(colors.accent)))
                        .text_color(rgb(if pending { colors.muted } else { colors.accent }))
                        .child("Refresh options")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if !this.create_pending {
                                this.request_create_config(&host_id, window, cx);
                                cx.notify();
                            }
                        })),
                ),
        )
        .when_some(config, |body, config| {
            if config.schema.properties.is_empty() {
                return body.child(
                    div()
                        .text_color(rgb(colors.muted))
                        .child("No additional host options."),
                );
            }
            // HashMap iteration order is unstable; sort by name so the
            // fields render in a deterministic order on every resolve.
            let mut properties: Vec<(
                &String,
                &ahp::ahp_types::state::SessionConfigPropertySchema,
            )> = config.schema.properties.iter().collect();
            properties.sort_by(|left, right| left.0.cmp(right.0));
            body.children(properties.into_iter().enumerate().map(
                |(index, (name, property))| {
                    let required = config
                        .schema
                        .required
                        .as_ref()
                        .is_some_and(|required| required.iter().any(|item| item == name));
                    let title = if property.title.is_empty() {
                        name.clone()
                    } else {
                        property.title.clone()
                    };
                    if property.read_only == Some(true) {
                        return div()
                            .id(("create-option-managed", index))
                            .text_color(rgb(colors.muted))
                            .child(format!(
                                "{title} · Managed by host{}",
                                if required { " · Required" } else { "" }
                            ))
                            .when_some(
                                property.description.as_ref(),
                                |body, description| {
                                    body.child(description.clone())
                                },
                            );
                    }
                    // Mirror the web ConfigField: non-editable types render
                    // as a static row with no input.
                    if !matches!(
                        property.r#type.as_str(),
                        "string" | "number" | "boolean" | "array" | "object"
                    ) {
                        return div()
                            .id(("create-option-managed", index))
                            .text_color(rgb(colors.muted))
                            .child(format!(
                                "{title} · Advanced host setting{}",
                                if required { " · Required" } else { "" }
                            ))
                            .when_some(
                                property.description.as_ref(),
                                |body, description| {
                                    body.child(description.clone())
                                },
                            );
                    }
                    // Mirror the web ConfigField: a static enum selects by
                    // index and a plain boolean selects Yes/No, both with
                    // keyboard-focusable options like the chat input selects.
                    if config_is_static_enum(property) {
                        let placeholder = if required {
                            "Choose an option"
                        } else {
                            "Host default"
                        };
                        let choices_len = property
                            .r#enum
                            .as_ref()
                            .map(Vec::len)
                            .unwrap_or(0);
                        let selected = state
                            .create_config_enums
                            .get(name)
                            .copied()
                            .flatten();
                        let name_key = name.clone();
                        let default_key = create_config_option_key(name, "default");
                        let default_focus = state
                            .create_config_option_focus
                            .get(&default_key)
                            .unwrap_or(&state.create_submit_focus);
                        let default_selected = selected.is_none();
                        let name_default = name_key.clone();
                        let key_default = default_key.clone();
                        let mut body = div()
                            .id(("create-option", index))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(format!(
                                "{title}{}",
                                if required { " · Required" } else { "" }
                            ))
                            .child(
                                div()
                                    .id(("create-option-default", index))
                                    .track_focus(default_focus)
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .text_color(rgb(if pending {
                                        colors.muted
                                    } else if default_selected {
                                        colors.accent
                                    } else {
                                        colors.text
                                    }))
                                    .child(format!(
                                        "{} {placeholder}",
                                        if default_selected { "●" } else { "○" }
                                    ))
                                    .when(!pending, |body| {
                                        body.on_click(cx.listener(move |this, _, _, cx| {
                                            this.create_config_enums.insert(name_default.clone(), None);
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                                                this.create_config_enums.insert(key_default.clone(), None);
                                                cx.stop_propagation();
                                                cx.notify();
                                            }
                                        }))
                                    }),
                            );
                        for option_index in 0..choices_len {
                            let label = config_enum_label(property, option_index);
                            let option_selected = selected == Some(option_index);
                            let option_key =
                                create_config_option_key(&name_key, &option_index.to_string());
                            let option_focus = state
                                .create_config_option_focus
                                .get(&option_key)
                                .unwrap_or(&state.create_submit_focus);
                            let name_option = name_key.clone();
                            let key_option = option_key.clone();
                            body = body.child(
                                div()
                                    .id(gpui::SharedString::from(format!(
                                        "create-option-choice-{index}-{option_index}"
                                    )))
                                    .track_focus(option_focus)
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .text_color(rgb(if pending {
                                        colors.muted
                                    } else if option_selected {
                                        colors.accent
                                    } else {
                                        colors.text
                                    }))
                                    .child(format!(
                                        "{} {label}",
                                        if option_selected { "●" } else { "○" }
                                    ))
                                    .when(!pending, |body| {
                                        body.on_click(cx.listener(move |this, _, _, cx| {
                                            this.create_config_enums.insert(
                                                name_option.clone(),
                                                Some(option_index),
                                            );
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                                                this.create_config_enums.insert(
                                                    key_option.clone(),
                                                    Some(option_index),
                                                );
                                                cx.stop_propagation();
                                                cx.notify();
                                            }
                                        }))
                                    }),
                            );
                        }
                        body = body.when_some(
                            property.description.as_ref(),
                            |body, description| {
                                body.child(
                                    div().text_color(rgb(colors.muted)).child(description.clone()),
                                )
                            },
                        );
                        return body;
                    }
                    if property.r#type == "boolean" {
                        let placeholder = if required {
                            "Choose an option"
                        } else {
                            "Host default"
                        };
                        let selected =
                            state.create_config_bools.get(name).copied().flatten();
                        let name_key = name.clone();
                        let mut body = div()
                            .id(("create-option", index))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(format!(
                                "{title}{}",
                                if required { " · Required" } else { "" }
                            ));
                        for (suffix, label, value) in
                            [("default", placeholder.to_owned(), None), ("true", "Yes".to_owned(), Some(true)), ("false", "No".to_owned(), Some(false))]
                        {
                            let option_key = create_config_option_key(&name_key, suffix);
                            let option_focus = state
                                .create_config_option_focus
                                .get(&option_key)
                                .unwrap_or(&state.create_submit_focus);
                            let option_selected = selected == value;
                            let name_option = name_key.clone();
                            let key_option = option_key.clone();
                            body = body.child(
                                div()
                                    .id(gpui::SharedString::from(format!(
                                        "create-option-bool-{index}-{suffix}"
                                    )))
                                    .track_focus(option_focus)
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .text_color(rgb(if pending {
                                        colors.muted
                                    } else if option_selected {
                                        colors.accent
                                    } else {
                                        colors.text
                                    }))
                                    .child(format!(
                                        "{} {label}",
                                        if option_selected { "●" } else { "○" }
                                    ))
                                    .when(!pending, |body| {
                                        body.on_click(cx.listener(move |this, _, _, cx| {
                                            this.create_config_bools.insert(name_option.clone(), value);
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                                                this.create_config_bools.insert(key_option.clone(), value);
                                                cx.stop_propagation();
                                                cx.notify();
                                            }
                                        }))
                                    }),
                            );
                        }
                        body = body.when_some(
                            property.description.as_ref(),
                            |body, description| {
                                body.child(
                                    div().text_color(rgb(colors.muted)).child(description.clone()),
                                )
                            },
                        );
                        return body;
                    }
                    let Some(input) = state.create_config_inputs.get(name) else {
                        return div().id(("create-option-missing", index)).child(title);
                    };
                    div()
                        .id(("create-option", index))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(format!(
                            "{}{}",
                            title,
                            if required { " · Required" } else { "" }
                        ))
                        .child(Input::new(input).disabled(pending))
                        .when_some(property.description.as_ref(), |body, description| {
                            body.child(div().text_color(rgb(colors.muted)).child(description.clone()))
                        })
                },
            ))
        })
        .when(config.is_none(), |body| {
            body.child(
                div()
                    .text_color(rgb(colors.muted))
                    .child("Host options not loaded. Refresh to request them."),
            )
        })
        .when(host.agents.is_empty(), |body| {
            body.child("The selected host currently advertises no agents.")
        })
        .child(
            div()
                .id("create-live-session")
                .track_focus(&state.create_submit_focus)
                .focus(|style| style.text_color(rgb(colors.accent)))
                .text_color(rgb(if pending { colors.muted } else { colors.accent }))
                .child(if pending {
                    "Creating session…"
                } else {
                    "Create session"
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.submit_create_session(window, cx);
                })),
        )
        .child(
            div()
                .text_color(rgb(colors.muted))
                .child(format!("Provider: {provider}")),
        )
}

fn input_question_view(
    request_id: &str,
    question: &ahp::ahp_types::state::ChatInputQuestion,
    state: &Relay,
    cx: &mut Context<Relay>,
) -> AnyElement {
    let colors = theme::Theme::global(cx).colors();
    use ahp::ahp_types::state::ChatInputQuestion;
    let (id, title, message): (&str, Option<&str>, &str) = match question {
        ChatInputQuestion::Text(question) => {
            (&question.id, question.title.as_deref(), &question.message)
        }
        ChatInputQuestion::Number(question) | ChatInputQuestion::Integer(question) => {
            (&question.id, question.title.as_deref(), &question.message)
        }
        ChatInputQuestion::Boolean(question) => {
            (&question.id, question.title.as_deref(), &question.message)
        }
        ChatInputQuestion::SingleSelect(question) => {
            (&question.id, question.title.as_deref(), &question.message)
        }
        ChatInputQuestion::MultiSelect(question) => {
            (&question.id, question.title.as_deref(), &question.message)
        }
        ChatInputQuestion::Unknown(value) => (
            unknown_question_id(value),
            unknown_question_title(value),
            unknown_question_message(value),
        ),
    };
    let key = input_question_key(request_id, id);
    let label = title
        .filter(|title| !title.is_empty())
        .unwrap_or(message)
        .to_owned();
    let focus = state.input_focus.get(&key);
    let mut body = div()
        .id(gpui::SharedString::from(format!("input-question-{key}")))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(label)
        .child(
            div()
                .text_color(rgb(colors.muted))
                .child(message.to_owned()),
        );
    match question {
        ChatInputQuestion::Text(_)
        | ChatInputQuestion::Number(_)
        | ChatInputQuestion::Integer(_)
        | ChatInputQuestion::Unknown(_) => {
            if let Some(input) = state.input_inputs.get(&key) {
                body = body.child(Input::new(input).appearance(false).bordered(true));
            }
        }
        ChatInputQuestion::Boolean(_) => {
            // `None` is the unanswered state: mirror the web select's
            // `Choose an answer` placeholder instead of showing `No`.
            let selected = state.input_booleans.get(&key).copied();
            let toggle_key = key.clone();
            body = body.child(
                div()
                    .id(gpui::SharedString::from(format!("input-boolean-{key}")))
                    .track_focus(focus.unwrap_or(&state.queue_turn_focus))
                    .focus(|style| style.text_color(rgb(colors.accent)))
                    .text_color(rgb(if selected == Some(true) {
                        colors.accent
                    } else {
                        colors.text
                    }))
                    .child(format!(
                        "{} {}",
                        if selected == Some(true) { "☑" } else { "☐" },
                        match selected {
                            Some(true) => "Yes",
                            Some(false) => "No",
                            None => "Choose an answer",
                        }
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let value = this
                            .input_booleans
                            .entry(toggle_key.clone())
                            .or_insert(false);
                        *value = !*value;
                        cx.notify();
                    })),
            );
        }
        ChatInputQuestion::SingleSelect(question) => {
            let selected = state
                .input_selections
                .get(&key)
                .and_then(|values| values.first())
                .cloned();
            body = body.children(question.options.iter().enumerate().map(|(index, option)| {
                let option_id = option.id.clone();
                let toggle_key = key.clone();
                let option_focus_key = input_option_key(request_id, id, &option.id);
                let option_focus = state
                    .input_option_focus
                    .get(&option_focus_key)
                    .unwrap_or(focus.unwrap_or(&state.queue_turn_focus));
                let selected = selected.as_deref() == Some(option.id.as_str());
                let click_toggle_key = toggle_key.clone();
                let click_option_id = option_id.clone();
                let key_toggle_key = toggle_key;
                let key_option_id = option_id;
                div()
                    .id(gpui::SharedString::from(format!(
                        "input-option-{key}-{index}"
                    )))
                    .track_focus(option_focus)
                    .focus(|style| style.text_color(rgb(colors.accent)))
                    .text_color(rgb(if selected { colors.accent } else { colors.text }))
                    .child(format!(
                        "{} {}",
                        if selected { "●" } else { "○" },
                        option.label
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.input_selections
                            .insert(click_toggle_key.clone(), vec![click_option_id.clone()]);
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                            this.input_selections
                                .insert(key_toggle_key.clone(), vec![key_option_id.clone()]);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
            }));
            if question.allow_freeform_input.unwrap_or(false) {
                body = body
                    .child(div().text_color(rgb(colors.muted)).child("Other response"))
                    .when_some(state.input_freeform.get(&key), |body, input| {
                        body.child(Input::new(input).appearance(false).bordered(true))
                    });
            }
        }
        ChatInputQuestion::MultiSelect(question) => {
            let selected = state
                .input_selections
                .get(&key)
                .cloned()
                .unwrap_or_default();
            body = body.children(question.options.iter().enumerate().map(|(index, option)| {
                let option_id = option.id.clone();
                let toggle_key = key.clone();
                let option_focus_key = input_option_key(request_id, id, &option.id);
                let option_focus = state
                    .input_option_focus
                    .get(&option_focus_key)
                    .unwrap_or(focus.unwrap_or(&state.queue_turn_focus));
                let selected = selected.iter().any(|value| value == &option.id);
                let click_toggle_key = toggle_key.clone();
                let click_option_id = option_id.clone();
                let key_toggle_key = toggle_key;
                let key_option_id = option_id;
                div()
                    .id(gpui::SharedString::from(format!(
                        "input-option-{key}-{index}"
                    )))
                    .track_focus(option_focus)
                    .focus(|style| style.text_color(rgb(colors.accent)))
                    .text_color(rgb(if selected { colors.accent } else { colors.text }))
                    .child(format!(
                        "{} {}",
                        if selected { "☑" } else { "☐" },
                        option.label
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let values = this
                            .input_selections
                            .entry(click_toggle_key.clone())
                            .or_default();
                        if let Some(index) =
                            values.iter().position(|value| value == &click_option_id)
                        {
                            values.remove(index);
                        } else {
                            values.push(click_option_id.clone());
                        }
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                            let values = this
                                .input_selections
                                .entry(key_toggle_key.clone())
                                .or_default();
                            if let Some(index) =
                                values.iter().position(|value| value == &key_option_id)
                            {
                                values.remove(index);
                            } else {
                                values.push(key_option_id.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
            }));
            if question.allow_freeform_input.unwrap_or(false) {
                body = body
                    .child(div().text_color(rgb(colors.muted)).child("Other response"))
                    .when_some(state.input_freeform.get(&key), |body, input| {
                        body.child(Input::new(input).appearance(false).bordered(true))
                    });
            }
        }
    }
    body.into_any_element()
}

fn live_input_request(
    request: &live::ChatInputRequestView,
    state: &Relay,
    cx: &mut Context<Relay>,
) -> AnyElement {
    let colors = theme::Theme::global(cx).colors();
    let pending = state.pending_input_submission.as_deref() == Some(request.request.id.as_str());
    let request_id = request.request.id.clone();
    let accept_request_id = request_id.clone();
    let decline_request_id = request_id.clone();
    let cancel_request_id = request_id.clone();
    let questions = request.request.questions.as_deref().unwrap_or(&[]);
    let review_url = request
        .request
        .url
        .as_ref()
        .map(ToString::to_string)
        .filter(|url| {
            Url::parse(url)
                .ok()
                .is_some_and(|parsed| matches!(parsed.scheme(), "http" | "https"))
        });
    div()
        .id(gpui::SharedString::from(format!(
            "live-input-request-{}",
            request.id
        )))
        .border_1()
        .border_color(rgb(colors.separator))
        .p(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(div().text_color(rgb(colors.warning)).child(if pending {
            "Input requested · submitting"
        } else {
            "Input requested"
        }))
        .child(
            request
                .request
                .message
                .clone()
                .filter(|message| !message.is_empty())
                .unwrap_or_else(|| "Agent requires input".to_owned()),
        )
        .when_some(review_url, |body, url| {
            body.child(
                div()
                    .text_color(rgb(colors.muted))
                    .child(format!("Review requested link: {url}")),
            )
        })
        .children(
            questions
                .iter()
                .map(|question| input_question_view(&request_id, question, state, cx)),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "accept-input-{}",
                            request.id
                        )))
                        .focus(|style| style.text_color(rgb(colors.accent)))
                        .text_color(rgb(if pending { colors.muted } else { colors.accent }))
                        .child(if pending {
                            "Submitting…"
                        } else {
                            "Submit answers"
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.pending_input_submission.is_none() {
                                this.answer_live_input(
                                    &accept_request_id,
                                    ahp::ahp_types::state::ChatInputResponseKind::Accept,
                                    window,
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "decline-input-{}",
                            request.id
                        )))
                        .focus(|style| style.text_color(rgb(colors.danger)))
                        .text_color(rgb(if pending { colors.muted } else { colors.danger }))
                        .child("Decline")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.pending_input_submission.is_none() {
                                this.answer_live_input(
                                    &decline_request_id,
                                    ahp::ahp_types::state::ChatInputResponseKind::Decline,
                                    window,
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "cancel-input-{}",
                            request.id
                        )))
                        .focus(|style| style.text_color(rgb(colors.muted)))
                        .text_color(rgb(if pending { colors.muted } else { colors.text }))
                        .child("Cancel")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.pending_input_submission.is_none() {
                                this.answer_live_input(
                                    &cancel_request_id,
                                    ahp::ahp_types::state::ChatInputResponseKind::Cancel,
                                    window,
                                    cx,
                                );
                            }
                        })),
                ),
        )
        .into_any_element()
}

fn response_markdown(colors: theme::Colors, id: usize, text: String) -> impl IntoElement {
    div()
        .id(("response-markdown", id))
        .max_h(px(400.0))
        .overflow_y_scroll()
        .font_family("monospace")
        .text_size(px(12.0))
        .line_height(px(20.4))
        .bg(rgb(colors.bg))
        .border_1()
        .border_color(rgb(colors.separator))
        .p(px(16.0))
        .child(text)
}

fn file_preview_text(
    colors: theme::Colors,
    id: &'static str,
    index: usize,
    text: &str,
) -> impl IntoElement {
    div()
        .id((id, index))
        .max_h(px(400.0))
        .overflow_y_scroll()
        .border_1()
        .border_color(rgb(colors.separator))
        .bg(rgb(colors.bg))
        .p(px(16.0))
        .font_family("monospace")
        .text_size(px(12.0))
        .line_height(px(20.4))
        .child(text.to_owned())
}

fn snapshot_turn(
    colors: theme::Colors,
    id: String,
    message: &str,
    parts: &[ahp::ahp_types::state::ResponsePart],
    status: &'static str,
    started_at: Option<&str>,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(id))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(rgb(colors.separator))
        .child(
            div()
                .text_color(rgb(colors.muted))
                .child(format!("Prompt · {}", live::last_activity(started_at))),
        )
        .child(if message.is_empty() { "—" } else { message }.to_owned())
        .children(parts.iter().enumerate().filter_map(|(index, part)| {
            let text = conversation::response_text(part)?;
            Some(match part {
                ahp::ahp_types::state::ResponsePart::Markdown(_) => {
                    response_markdown(colors, index, text).into_any_element()
                }
                _ => div().child(text).into_any_element(),
            })
        }))
        .child(div().text_color(rgb(colors.muted)).child(status))
}

fn session_tabs(state: &Relay, cx: &mut Context<Relay>) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    div()
        .flex()
        .gap(px(16.0))
        .border_b_1()
        .border_color(rgb(colors.separator))
        .children(
            SESSION_TABS
                .into_iter()
                .enumerate()
                .map(|(index, (tab, label))| {
                    div()
                        .id(("session-tab", index))
                        .track_focus(
                            &state.session_tab_focus[index]
                                .clone()
                                .tab_stop(state.session_tab == tab),
                        )
                        .focus(|style| style.bg(rgb(colors.separator)))
                        .text_color(rgb(if state.session_tab == tab {
                            colors.accent
                        } else {
                            colors.muted
                        }))
                        .py(px(8.0))
                        .child(label)
                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                            let next = match event.keystroke.key.as_str() {
                                "left" | "up" => {
                                    (index + SESSION_TABS.len() - 1) % SESSION_TABS.len()
                                }
                                "right" | "down" => (index + 1) % SESSION_TABS.len(),
                                "home" => 0,
                                "end" => SESSION_TABS.len() - 1,
                                _ => return,
                            };
                            this.session_tab = SESSION_TABS[next].0;
                            this.load_files_if_needed();
                            this.session_tab_focus[next].focus(window);
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.session_tab = tab;
                            this.load_files_if_needed();
                            this.session_tab_focus[index].focus(window);
                            cx.notify();
                        }))
                }),
        )
}

fn live_session_details(
    host: Option<&live::HostView>,
    uri: &str,
    state: &Relay,
    cx: &mut Context<Relay>,
) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    let session = host.and_then(|host| host.sessions.iter().find(|session| session.uri == uri));
    let snapshot = state
        .session_snapshot
        .as_ref()
        .and_then(|result| result.as_ref().ok());
    let host_name = host
        .map(|host| host.label.as_str())
        .filter(|label| !label.is_empty())
        .or(match &state.drawer {
            Some(NodeKey::Session(host, _)) => Some(host.as_str()),
            _ => None,
        })
        .unwrap_or("Host");
    let provider = session
        .map(|session| session.provider.as_str())
        .filter(|provider| !provider.is_empty())
        .or_else(|| {
            snapshot
                .map(|snapshot| snapshot.provider.as_str())
                .filter(|provider| !provider.is_empty())
        })
        .unwrap_or("SESSION");
    let title = session
        .map(|session| session.title.as_str())
        .filter(|title| !title.is_empty())
        .or_else(|| {
            snapshot
                .map(|snapshot| snapshot.title.as_str())
                .filter(|title| !title.is_empty())
        })
        .unwrap_or(
            if state.session_snapshot.as_ref().is_some_and(Result::is_err) {
                "Session unavailable"
            } else {
                "Loading session"
            },
        );
    let workspace = live::info_workspace(snapshot, session).to_owned();
    let disposal_pending = state.pending_session_disposal.is_some();
    let disposal_confirm_open = state.dispose_confirm_open && !disposal_pending;
    div()
        .occlude().absolute().top(px(36.0)).right(px(0.0)).bottom(px(0.0))
        .w(px(352.0)).max_w_full().flex().flex_col().bg(rgb(colors.surface))
        .border_l_1().border_color(rgb(colors.separator))
        .child(div().h(px(56.0)).px(px(16.0)).flex().items_center().justify_between()
            .border_b_1().border_color(rgb(colors.separator)).child("SESSION")
            .child(div().id("close-live-session").track_focus(&state.drawer_close_focus)
                .focus(|style| style.text_color(rgb(colors.accent)))
                .text_color(rgb(if disposal_pending { colors.muted } else { colors.accent }))
                .child(if disposal_pending { "…" } else { "×" })
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.pending_session_disposal.is_none() {
                        this.close_drawer(window);
                        cx.notify();
                    }
                }))))
        .child(div().id("live-session-content").flex_1().min_h_0().overflow_y_scroll()
            .p(px(16.0)).flex().flex_col().gap(px(12.0))
            .child(div().text_color(rgb(colors.muted)).child(format!("{host_name} / {provider}")))
            .child(div().text_size(px(16.0)).child(title.to_owned()))
            .when_some(snapshot.filter(|snapshot| snapshot.lifecycle == "failed"), |body, snapshot| {
                let message = conversation::safe_error(snapshot.creation_error.as_deref().unwrap_or(""), "The host could not initialize this session.");
                body.child(div().text_color(rgb(colors.danger)).child(message))
            })
            .child(session_tabs(state, cx))
            .when(state.session_tab == SessionTab::Conversation, |body| body
            .child(div().id("refresh-session-snapshot").track_focus(&state.snapshot_refresh_focus)
                .focus(|style| style.text_color(rgb(colors.accent)))
                .child(if state.session_snapshot.is_none() { "Refreshing snapshot…" } else { "Refresh snapshot" })
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.session_snapshot.is_some()
                        && let Some(key @ NodeKey::Session(_, _)) = this.drawer.clone()
                    {
                        if this.chat_focus.values().any(|focus| focus.is_focused(window)) {
                            this.snapshot_refresh_focus.focus(window);
                        }
                        this.refresh_snapshot(&key, live::ChatSelection::Preferred(this.selected_chat.clone()), live::ChangesetSelection::Preferred(this.selected_changeset.clone()));
                        cx.notify();
                    }
                })))
            .child(match &state.session_snapshot {
                None => div().text_color(rgb(colors.muted)).child("Loading session snapshot…"),
                Some(Err(error)) => div().text_color(rgb(colors.muted)).child(conversation::snapshot_error(error)),
                Some(Ok(snapshot)) => {
                    let input_allowed = snapshot
                        .conversation
                        .as_ref()
                        .ok()
                        .and_then(|chat| chat.as_ref())
                        .is_some_and(|chat| {
                            !matches!(
                                chat.interactivity,
                                Some(
                                    ahp::ahp_types::state::ChatInteractivity::ReadOnly
                                        | ahp::ahp_types::state::ChatInteractivity::Hidden
                                )
                            )
                        });
                    let prompt_blocked: Option<&'static str> =
                        match snapshot.lifecycle.as_str() {
                            "failed" => Some("Session creation failed on the host"),
                            "creating" => Some("Session is still starting"),
                            _ => None,
                        };
                    // Mirror fleet.chatPromptPolicy: failed/creating reasons win,
                    // then a present but non-writable chat reports its own
                    // interactivity with the source's verbatim interpolation.
                    let prompt_policy_reason: Option<&'static str> = match snapshot
                        .conversation
                        .as_ref()
                        .ok()
                        .and_then(|conversation| conversation.as_ref())
                    {
                        Some(chat)
                            if matches!(
                                chat.interactivity,
                                Some(
                                    ahp::ahp_types::state::ChatInteractivity::ReadOnly
                                        | ahp::ahp_types::state::ChatInteractivity::Hidden
                                )
                            ) =>
                        {
                            Some(match chat.interactivity {
                                Some(ahp::ahp_types::state::ChatInteractivity::Hidden) => {
                                    "This chat is hidden. The host does not accept user prompts here."
                                }
                                _ => {
                                    "This chat is read-only. The host does not accept user prompts here."
                                }
                            })
                        }
                        _ => None,
                    };
                    let pending_submission = state.pending_turn_submission.is_some();
                    let model_options = host
                        .and_then(|host| {
                            host.agents
                                .iter()
                                .find(|agent| agent.provider == snapshot.provider)
                        })
                        .map(|agent| agent.models.as_slice())
                        .filter(|models| !models.is_empty());
                    let model_selector = model_options.map(|models| {
                        let host_id = match &state.drawer {
                            Some(NodeKey::Session(host, _)) => host.as_str(),
                            _ => "",
                        };
                        let provider = snapshot.provider.as_str();
                        let default_key = live_model_focus_key(host_id, provider, DEFAULT_MODEL_ID);
                        let default_focus = state.live_model_focus.get(&default_key);
                        let default_control = default_focus.map(|focus| {
                            div()
                                .id("live-model-default")
                                .track_focus(focus)
                                .focus(|style| style.text_color(rgb(colors.accent)))
                                .text_color(rgb(if pending_submission { colors.muted } else { colors.accent }))
                                .child(format!(
                                    "{} Host default",
                                    if state.live_model.is_none() { "●" } else { "○" }
                                ))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.pending_turn_submission.is_none() {
                                        this.live_model = None;
                                        cx.notify();
                                    }
                                }))
                        });
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(div().text_color(rgb(colors.muted)).child("Model"))
                            .children(default_control)
                            .children(models.iter().enumerate().filter_map(|(index, model)| {
                                let key = live_model_focus_key(host_id, provider, &model.id);
                                let focus = state.live_model_focus.get(&key)?;
                                let model_id = model.id.clone();
                                let selected = state.live_model.as_deref() == Some(model.id.as_str());
                                let disabled = pending_submission || model.disabled;
                                let label = if model.name.is_empty() {
                                    model.id.clone()
                                } else {
                                    model.name.clone()
                                };
                                let label = if model.unconfigured {
                                    format!("{label} · Not configured")
                                } else {
                                    label
                                };
                                Some(
                                    div()
                                        .id(("live-model", index))
                                        .track_focus(focus)
                                        .focus(|style| style.text_color(rgb(colors.accent)))
                                        .text_color(rgb(if disabled {
                                            colors.muted
                                        } else if selected {
                                            colors.accent
                                        } else {
                                            colors.text
                                        }))
                                        .child(format!("{} {label}", if selected { "●" } else { "○" }))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if !disabled {
                                                this.live_model = Some(model_id.clone());
                                                cx.notify();
                                            }
                                        })),
                                )
                            }))
                    });
                    div().flex().flex_col().gap(px(8.0))
                    .child(div().text_color(rgb(colors.muted)).child(if snapshot.changes.is_err() {
                        "Sampled · Files loads the changes"
                    } else {
                        "Host snapshot · not live; refresh to update"
                    }))
                    .when(!snapshot.attention.is_empty(), |body| body
                        .child(div().text_color(rgb(colors.warning)).child("Attention needed"))
                        .children(snapshot.attention.iter().map(|label| div().child(*label))))
                    .children(snapshot.input_requests.iter().map(|request| {
                        live_input_request(request, state, cx)
                    }))
                    .children(snapshot.auth_requests.iter().map(|request| {
                        // Mirror the web `Attention` fallthrough for kinds
                        // without an in-app action: the kind plus a
                        // resolve-in-host note.
                        div()
                            .border_1()
                            .border_color(rgb(colors.separator))
                            .p(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(div().text_size(px(16.0)).child(request.kind))
                            .child("Resolve this request in the agent host.")
                    }))
                    .child(format!("Snapshot status: {}", status::session_state(Some(i64::from(snapshot.status)), Some(&snapshot.lifecycle), host.is_some_and(|host| host.status == "connected"))))
                    .child(format!("Chats: {}", snapshot.chats))
                    .children(snapshot.working_directories.iter().map(|directory| div().child(format!("Workspace: {directory}"))))
                    .child(div().text_size(px(16.0)).child("Conversation"))
                    .when(snapshot.visible_chats.len() > 1, |body| {
                        body.child(div().text_color(rgb(colors.muted)).child("Chat"))
                            .children(snapshot.visible_chats.iter().enumerate().filter_map(|(index, (uri, title))| {
                                let focus = state.chat_focus.get(uri)?;
                                let selected = snapshot.selected_chat.as_deref() == Some(uri.as_str());
                                let target = uri.clone();
                                Some(div().id(("snapshot-chat", index)).track_focus(focus)
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .child(format!("{} {}", if selected { "●" } else { "○" }, if title.is_empty() { uri } else { title }))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if this.session_snapshot.is_some() && this.selected_chat.as_deref() != Some(target.as_str())
                                            && let Some(key @ NodeKey::Session(_, _)) = this.drawer.clone()
                                        {
                                            this.snapshot_refresh_focus.focus(window);
                                            this.refresh_snapshot(&key, live::ChatSelection::Explicit(target.clone()), live::ChangesetSelection::Preferred(this.selected_changeset.clone()));
                                            cx.notify();
                                        }
                                    })))
                            }))
                    })
                    .when_some(snapshot.approval.as_ref(), |body, approval| {
                        let pending = state.pending_tool_confirmation.as_ref().is_some_and(|pending| {
                            pending.turn_id == approval.turn_id
                                && pending.tool_call_id == approval.tool_call_id
                        });
                        body.child(div().text_size(px(16.0)).child(approval.title.clone()))
                        .child(approval.intention.clone())
                        .when(!approval.parameters.is_empty(), |body| {
                            body.child(div().text_color(rgb(colors.muted)).child("Tool parameters"))
                                .child(approval.parameters.clone())
                        })
                        .child(div().flex().gap(px(12.0)).children(
                            approval.actions.iter().enumerate().map(|(index, action)| {
                                let approved = action.approved;
                                let option_id = action.option_id.clone();
                                let disabled = pending || approval.actions_disabled;
                                let label = if pending {
                                    if approved { "Submitting…" } else { "Waiting…" }.to_owned()
                                } else {
                                    action.label.clone()
                                };
                                div()
                                    .id(if approved { "approve-live-tool" } else { "deny-live-tool" })
                                    .track_focus(if index == 0 {
                                        &state.approval_focus
                                    } else {
                                        &state.approval_second_focus
                                    })
                                    .focus(|style| {
                                        style.text_color(rgb(if approved {
                                            colors.accent
                                        } else {
                                            colors.danger
                                        }))
                                    })
                                    .text_color(rgb(if disabled {
                                        colors.muted
                                    } else if approved {
                                        colors.accent
                                    } else {
                                        colors.danger
                                    }))
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if !disabled {
                                            this.confirm_live_tool(
                                                approved,
                                                option_id.clone(),
                                                window,
                                                cx,
                                            );
                                        }
                                    }))
                            }),
                        ))
                    })
                    .when(input_allowed && prompt_blocked.is_none(), |body| {
                        body.when_some(model_selector, |body, selector| body.child(selector))
                            .child(div().text_color(rgb(colors.muted)).child("Message / queued turn"))
                            .child(
                                Input::new(&state.live_prompt)
                                    .appearance(false)
                                    .bordered(false)
                                    .disabled(pending_submission),
                            )
                            .child(
                                div()
                                    .id("queue-live-turn")
                                    .track_focus(&state.queue_turn_focus)
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .text_color(rgb(if pending_submission { colors.muted } else { colors.accent }))
                                    .child(if pending_submission { "Queuing…" } else { "Queue turn" })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.queue_live_turn(window, cx);
                                    })),
                            )
                    })
                    .when_some(prompt_blocked.or(prompt_policy_reason), |body, reason| {
                        body.child(div().text_color(rgb(colors.muted)).child(reason.to_owned()))
                    })
                    .child(match &snapshot.conversation {
                        Err(error) => div().text_color(rgb(colors.muted)).child(format!("Conversation unavailable: {error}")),
                        Ok(None) => div().text_color(rgb(colors.muted)).child("Awaiting host chat."),
                        Ok(Some(chat)) => {
                            use ahp::ahp_types::state::TurnState;
                            div().flex().flex_col().gap(px(6.0))
                                .child(div().text_color(rgb(colors.muted)).child(format!("{} · point-in-time snapshot, not live", if chat.title.is_empty() { chat.resource.as_str() } else { &chat.title })))
                                .when_some(
                                    chat.turns_next_cursor.as_deref().filter(|cursor| !cursor.is_empty()),
                                    |body, cursor| {
                                        let pending = state.pending_turn_page.as_ref().is_some_and(|pending| {
                                            pending.chat == chat.resource.as_str()
                                                && pending.cursor == cursor
                                        });
                                        body.child(
                                                div()
                                                    .id("load-older-turns")
                                                    .track_focus(&state.snapshot_refresh_focus)
                                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                                    .text_color(rgb(if pending { colors.muted } else { colors.accent }))
                                                    .child(if pending { "Loading earlier turns…" } else { "Load earlier turns" })
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.load_older_live_turns(window, cx);
                                                    })),
                                            )
                                    },
                                )
                                .children(chat.turns.iter().map(|turn| snapshot_turn(colors, format!("completed-turn-{}", turn.id), &turn.message.text, &turn.response_parts, match turn.state {
                                    TurnState::Complete if turn.response_parts.is_empty() => "Host reports complete · no response recorded",
                                    TurnState::Complete => "complete",
                                    TurnState::Cancelled => "cancelled",
                                    TurnState::Error => "error",
                                }, turn.started_at.as_deref())))
                                .when_some(chat.active_turn.as_ref(), |body, turn| {
                                    let turn_id = turn.id.clone();
                                    let pending = state
                                        .pending_turn_cancellation
                                        .as_ref()
                                        .is_some_and(|pending| pending.turn_id == turn_id);
                                    body.child(snapshot_turn(colors,
                                        format!("active-turn-{}", turn.id),
                                        &turn.message.text,
                                        &turn.response_parts,
                                        if pending {
                                            "Cancellation requested · waiting for host"
                                        } else {
                                            "In progress"
                                        },
                                        Some(&turn.started_at),
                                    ))
                                    .child(div()
                                        .id("cancel-live-turn")
                                        .track_focus(&state.cancel_turn_focus)
                                        .focus(|style| style.text_color(rgb(colors.danger)))
                                        .text_color(rgb(if pending { colors.muted } else { colors.danger }))
                                        .child(if pending { "Cancelling…" } else { "Cancel turn" })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.cancel_live_turn(window, cx);
                                        })))
                                })
                                .when(chat.turns.is_empty() && chat.active_turn.is_none(), |body| body.child(div().text_color(rgb(colors.muted)).child("No turns in this chat.")))
                                .when_some(chat.queued_messages.as_ref().filter(|messages| !messages.is_empty()), |body, messages| body.child(format!("{} queued turn(s)", messages.len())))
                                .when(pending_submission, |body| body.child(div().text_color(rgb(colors.muted)).child("Awaiting host echo · do not retry")))
                        }
                    })
            }}))
            .when(state.session_tab == SessionTab::Files, |body| body
                .child("Files / changes")
                .child(div().id("refresh-files-snapshot").track_focus(&state.snapshot_refresh_focus)
                    .focus(|style| style.text_color(rgb(colors.accent)))
                    .child(if state.session_snapshot.is_none() { "Refreshing snapshot…" } else { "Refresh snapshot" })
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.session_snapshot.is_some()
                            && let Some(key @ NodeKey::Session(_, _)) = this.drawer.clone()
                        {
                            if this.changeset_focus.values().any(|focus| focus.is_focused(window))
                                || this.changeset_param_focus.values().any(|focus| focus.is_focused(window))
                                || this.live_file_focus.values().any(|focus| focus.is_focused(window))
                            {
                                this.snapshot_refresh_focus.focus(window);
                            }
                            this.refresh_snapshot(&key, live::ChatSelection::Preferred(this.selected_chat.clone()), live::ChangesetSelection::Preferred(this.selected_changeset.clone()));
                            cx.notify();
                        }
                    })))
                .child(match &state.session_snapshot {
                    None => div().text_color(rgb(colors.muted)).child("Loading snapshot or reading file content…"),
                    Some(Err(error)) => div().text_color(rgb(colors.muted)).child(conversation::snapshot_error(error)),
                    Some(Ok(snapshot)) => {
                        let mut changeset_parameter_controls: Vec<AnyElement> = Vec::new();
                        for (template_uri, template) in &snapshot.changeset_templates {
                            for parameter in live::changeset_parameter_names(template) {
                                let key = changeset_parameter_key(template_uri, parameter);
                                let selected = state
                                    .changeset_params
                                    .get(&key)
                                    .cloned()
                                    .or_else(|| default_changeset_parameter(snapshot, parameter));
                                if let Some(focus) = state.changeset_param_focus.get(&key) {
                                    for (turn_index, (turn_id, _)) in snapshot.changeset_turns.iter().enumerate() {
                                        let value = turn_id.clone();
                                        let selected_value = selected.as_deref() == Some(turn_id.as_str());
                                        let label = changeset_turn_label(snapshot, turn_index, turn_id);
                                        let template_uri = template_uri.clone();
                                        let template = template.clone();
                                        let parameter = parameter.to_owned();
                                        let changeset_label = snapshot
                                            .available_changesets
                                            .iter()
                                            .find(|(uri, _)| uri == &template_uri)
                                            .map(|(_, label)| label.as_str())
                                            .unwrap_or("Changeset");
                                        changeset_parameter_controls.push(
                                            div()
                                                .id(("changeset-parameter", changeset_parameter_controls.len() + turn_index))
                                                .track_focus(focus)
                                                .focus(|style| style.text_color(rgb(colors.accent)))
                                                .text_color(rgb(if selected_value { colors.accent } else { colors.text }))
                                                .child(format!("{} {} · {} · {}", if selected_value { "●" } else { "○" }, changeset_label, changeset_parameter_label(parameter.as_str()), label))
                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                    this.select_changeset_parameter(
                                                        template_uri.clone(),
                                                        template.clone(),
                                                        parameter.clone(),
                                                        value.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }))
                                                .into_any_element(),
                                        );
                                    }
                                }
                            }
                        }
                        div().flex().flex_col().gap(px(8.0))
                        .when(snapshot.available_changesets.len() > 1, |body| body
                            .child(div().text_color(rgb(colors.muted)).child("Changeset"))
                            .children(snapshot.available_changesets.iter().enumerate().filter_map(|(index, (uri, label))| {
                                let focus = state.changeset_focus.get(uri)?;
                                let selected = state.selected_changeset.as_deref() == Some(uri.as_str());
                                let target = uri.clone();
                                Some(div().id(("snapshot-changeset", index)).track_focus(focus)
                                    .focus(|style| style.bg(rgb(colors.separator)))
                                    .child(format!("{} {}", if selected { "●" } else { "○" }, label))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if this.session_snapshot.is_some()
                                            && (this.selected_changeset.as_deref() != Some(target.as_str())
                                                || matches!(&this.session_snapshot, Some(Ok(snapshot)) if snapshot.changes.is_err()))
                                            && let Some(key @ NodeKey::Session(_, _)) = this.drawer.clone()
                                        {
                                            this.session_tab_focus[1].focus(window);
                                            this.refresh_snapshot(&key, live::ChatSelection::Preferred(this.selected_chat.clone()), live::ChangesetSelection::Explicit(target.clone()));
                                            cx.notify();
                                        }
                                    })))
                            })))
                        .children(changeset_parameter_controls)
                                    .child(match &snapshot.changes {
                        Err(error) => div().text_color(rgb(colors.muted)).child(format!("Changeset unavailable: {error}")),
                        Ok(None) if snapshot.changeset_count > 0 => div().text_color(rgb(colors.muted)).child("Complete a turn to inspect host-advertised changes."),
                        Ok(None) => div().text_color(rgb(colors.muted)).child("Host advertises no file changes."),
                        Ok(Some((label, changes))) => {
                            use ahp::ahp_types::state::ChangesetStatus;
                            let status = match &changes.status {
                                ChangesetStatus::Computing => "computing",
                                ChangesetStatus::Recomputing => "recomputing",
                                ChangesetStatus::Ready => "ready",
                                ChangesetStatus::Error => "error",
                                ChangesetStatus::Unknown(value) => value,
                            };
                            div().flex().flex_col().gap(px(8.0))
                                .child(div().text_color(rgb(colors.muted)).child("Host snapshot · not live; refresh to update"))
                                .when(snapshot.conversation.is_err(), |body| body.child(div().text_color(rgb(colors.muted)).child("Changeset catalog unavailable")))
                                .child(format!("{label} · {status}"))
                                .when(matches!(changes.status, ChangesetStatus::Recomputing), |body| body.child("Recomputing; displaying previous result."))
                                .when_some(conversation::changeset_error(changes), |body, error| body.child(div().text_color(rgb(colors.danger)).child(error)))
                                .children(changes.files.iter().enumerate().filter_map(|(index, file)| {
                                    let path = file.edit.after.as_ref().map(|side| side.uri.as_str()).filter(|uri| !uri.is_empty())
                                        .or_else(|| file.edit.before.as_ref().map(|side| side.uri.as_str()).filter(|uri| !uri.is_empty()))
                                        .unwrap_or(&file.id);
                                    let focus = state.live_file_focus.get(&file.id)?;
                                    let file_id = file.id.clone();
                                    let preview_file_id = file_id.clone();
                                    let reviewed = file.reviewed.unwrap_or(false);
                                    let review_focus = state.live_review_focus.get(&file.id);
                                    let review_pending = state.pending_file_review.as_ref().is_some_and(|pending| {
                                        pending.file == file.id
                                            && pending.changeset == snapshot.selected_changeset.clone().unwrap_or_default()
                                    });
                                    let preview = snapshot.preview.as_ref().filter(|(id, _)| id == &file.id);
                                    let loading = state.pending_file_preview.as_deref() == Some(file.id.as_str());
                                    Some(div().border_b_1().border_color(rgb(colors.separator)).py(px(8.0))
                                        .child(div().text_color(rgb(colors.text)).child(path_tail(path, if path.is_empty() { "—" } else { path })))
                                        .child(path.to_owned())
                                        .when(snapshot.changeset_reviewable, |body| body
                                            .when_some(review_focus, |body, review_focus| body.child(
                                                div().id(("live-review", index)).track_focus(review_focus)
                                                    .focus(|style| style.bg(rgb(colors.separator)))
                                                    .text_color(rgb(colors.accent)).py(px(8.0))
                                                    .child(if review_pending { "Updating review…" } else if reviewed { "Mark not reviewed" } else { "Mark reviewed" })
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        if !review_pending {
                                                            this.review_live_file(file_id.clone(), !reviewed, window, cx);
                                                        }
                                                    })),
                                            )))
                                        .child(div().id(("live-file", index)).track_focus(focus)
                                            .focus(|style| style.bg(rgb(colors.separator)))
                                            .text_color(rgb(colors.accent)).py(px(8.0))
                                            .child(if preview.is_some() || loading { "Close preview" } else { "Inspect file" })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if this.pending_file_preview.as_deref() == Some(preview_file_id.as_str()) {
                                                    this.pending_file_preview = None;
                                                    this.snapshot_request = this.snapshot_request.wrapping_add(1);
                                                    if let Some(fleet) = &this.live_fleet {
                                                        fleet.invalidate_snapshot();
                                                    }
                                                } else if let Some(Ok(snapshot)) = &mut this.session_snapshot {
                                                    if snapshot.preview.as_ref().is_some_and(|(id, _)| id == &preview_file_id) {
                                                        snapshot.preview = None;
                                                    } else {
                                                        this.inspect_live_file(preview_file_id.clone());
                                                    }
                                                }
                                                cx.notify();
                                            })))
                                        .when(loading, |body| body.child(div().text_color(rgb(colors.muted)).child("Reading file content…")))
                                        .when_some(preview, |body, (_, result)| match result {
                                            Err(error) => body.child(div().text_color(rgb(colors.muted)).child(conversation::safe_error(error, "Unknown error"))),
                                            Ok(preview) => body
                                                .child("Before")
                                                .child(file_preview_text(colors, "before-file-preview", index, preview.before.as_deref().unwrap_or("No before content supplied.")))
                                                .child("After")
                                                .child(file_preview_text(colors, "after-file-preview", index, preview.after.as_deref().unwrap_or("No after content supplied."))),
                                        }))
                                }))
                                .when(changes.status == ChangesetStatus::Ready && changes.files.is_empty(), |body| body.child("No changed files reported."))
                                .child(div().text_color(rgb(colors.muted)).child("Live host only · refresh to confirm"))
                        }})
                    },
                }))
            .when(state.session_tab == SessionTab::Info, |body| body
            .when(state.session_snapshot.is_none(), |body| body
                .child(div().text_color(rgb(colors.muted)).child("Loading session snapshot…")))
            .when_some(state.session_snapshot.as_ref().and_then(|result| result.as_ref().err()), |body, error| body
                .child(div().text_color(rgb(colors.muted)).child(conversation::snapshot_error(error))))
            .child(format!("Session: {uri}"))
            .child(format!("Workspace: {workspace}"))
            .when_some(host.zip(session), |body, (host, session)| {
                // Mirror the web header: the cached summary has no lifecycle, so
                // prefer the authoritative snapshot only when it reports failure.
                let failed = snapshot.filter(|current| current.lifecycle == "failed");
                let (status_value, lifecycle) = match failed {
                    Some(current) => (
                        Some(i64::from(current.status)),
                        Some(current.lifecycle.as_str()),
                    ),
                    None => (Some(i64::from(session.status)), None),
                };
                body.child(div().text_size(px(16.0)).child(if session.title.is_empty() { "Untitled session" } else { &session.title }.to_owned()))
                    .child(format!("Host: {}", host.label))
                    .child(format!("Agent: {}", if session.provider.is_empty() { "—" } else { &session.provider }))
                    .child(format!(
                        "Status: {}",
                        status::session_state(
                            status_value,
                            lifecycle,
                            host.status == "connected"
                        )
                    ))
            })
            .child(format!("Last activity: {}", live::last_activity(session.map(|session| session.modified_at.as_str()))))
            .when(session.is_none(), |body| body.child("Not in the cached catalog"))
            .when(disposal_pending, |body| {
                body.child(div().text_color(rgb(colors.muted)).child("Disposing session…"))
            })
            .when(!disposal_pending && !disposal_confirm_open, |body| {
                body.child(
                    div()
                        .id("dispose-live-session")
                        .track_focus(&state.dispose_focus)
                        .focus(|style| style.text_color(rgb(colors.danger)))
                        .text_color(rgb(colors.danger))
                        .child("Dispose session")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.request_dispose_session(window, cx);
                        })),
                )
            })
            .when(disposal_confirm_open, |body| {
                body.child(div().text_color(rgb(colors.danger)).child("Dispose this AHP session? This cannot be undone."))
                    .child(
                        div()
                            .id("confirm-dispose-live-session")
                            .track_focus(&state.dispose_confirm_focus)
                            .focus(|style| style.text_color(rgb(colors.danger)))
                            .text_color(rgb(colors.danger))
                            .child("Confirm dispose")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dispose_live_session(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("cancel-dispose-live-session")
                            .track_focus(&state.dispose_cancel_focus)
                            .focus(|style| style.text_color(rgb(colors.accent)))
                            .text_color(rgb(colors.accent))
                            .child("Keep session")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel_dispose_session(window, cx);
                            })),
                    )
            })))
}

fn preview(node: &Node, state: &Relay, cx: &mut Context<Relay>) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    let kind = match node.kind {
        Kind::Host => "HOST",
        Kind::Provider => "PROVIDER",
        Kind::Session => "SESSION",
    };
    let can_control = !state.live
        || state.live_hosts.iter().any(|host| {
            host.id == node.host_id
                && (node.kind != Kind::Session
                    || host
                        .sessions
                        .iter()
                        .any(|session| node.uri.as_deref() == Some(session.uri.as_str())))
        });
    div()
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(12.0))
        .border_b_1()
        .border_color(rgb(colors.separator))
        .bg(rgb(colors.surface))
        .text_size(px(12.0))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(node.title.clone())
                .child(div().text_color(rgb(colors.muted)).child(kind)),
        )
        .child(format!("Status: {}", node.status))
        .when(node.kind != Kind::Host, |item| {
            item.child(format!("Host: {}", node.host))
        })
        .when(node.kind == Kind::Session, |item| {
            item.child(format!(
                "Agent: {}  ·  Workspace: {}",
                if node.provider.is_empty() {
                    "—"
                } else {
                    &node.provider
                },
                path_tail(
                    &node.workspace,
                    if node.workspace.is_empty() {
                        "—"
                    } else {
                        &node.workspace
                    }
                )
            ))
        })
        .when(node.kind != Kind::Session, |item| {
            item.child(format!("Sessions: {}", node.session_count))
        })
        .when(state.live, |item| {
            item.child(
                div()
                    .text_color(rgb(colors.muted))
                    .child("Cached at selection · freshness unverified"),
            )
        })
        .when(!can_control, |item| {
            item.child(
                div()
                    .text_color(rgb(colors.muted))
                    .child("Controls unavailable"),
            )
        })
        .when(can_control, |item| {
            item.child(
                div()
                    .id("open-details")
                    .track_focus(
                        &state
                            .preview_control_focus
                            .clone()
                            .tab_stop(state.drawer.is_none()),
                    )
                    .focus(|style| style.bg(rgb(colors.separator)))
                    .text_color(rgb(colors.accent))
                    .child(if state.live && node.kind == Kind::Session {
                        "Inspect →"
                    } else {
                        "Control →"
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(key) = this.selected.clone() {
                            this.open_drawer(key, this.preview_control_focus.clone(), window);
                            cx.notify();
                        }
                    })),
            )
        })
        .child(
            div()
                .id("close-preview")
                .track_focus(
                    &state
                        .preview_close_focus
                        .clone()
                        .tab_stop(state.drawer.is_none()),
                )
                .focus(|style| style.text_color(rgb(colors.accent)))
                .ml_auto()
                .child("×")
                .on_click(cx.listener(|this, _, window, cx| {
                    let preview_has_focus = this.preview_control_focus.is_focused(window)
                        || this.preview_close_focus.is_focused(window);
                    this.selected = None;
                    if preview_has_focus {
                        this.topology_focus.focus(window);
                    }
                    cx.notify();
                })),
        )
}

fn details(node: &Node, state: &Relay, cx: &mut Context<Relay>) -> impl IntoElement {
    let colors = theme::Theme::global(cx).colors();
    let approval_expanded = state.approval_expanded;
    let approval_focus = &state.approval_focus;
    let session_tab = state.session_tab;

    let file_preview = state.file_preview;
    let file_focus = &state.file_focus;
    let heading = match node.kind {
        Kind::Host => "HOST DETAILS",
        Kind::Provider => "PROVIDER",
        Kind::Session => "SESSION",
    };
    let session = node.uri.as_deref().and_then(|uri| {
        example::sessions().find(|session| session.host_id == node.host_id && session.uri == uri)
    });
    div()
        .occlude()
        .absolute()
        .top(px(36.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .w(px(352.0))
        .max_w_full()
        .flex()
        .flex_col()
        .bg(rgb(colors.surface))
        .border_l_1()
        .border_color(rgb(colors.separator))
        .child(
            div()
                .h(px(56.0))
                .px(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(rgb(colors.separator))
                .child(heading)
                .child(
                    div()
                        .id("close-details")
                        .track_focus(&state.drawer_close_focus)
                        .focus(|style| style.text_color(rgb(colors.accent)))
                        .child("×")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_drawer(window);
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .id("details-content")
                .flex_1()
                .overflow_y_scroll()
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(div().text_size(px(16.0)).child(node.title.clone()))
                .when(node.kind == Kind::Host, |body| {
                    body.child("Example host · not connected")
                        .child("Protocol: AHP 1.0")
                        .child(format!("Agents: {}", node.agent_count))
                        .child(format!("Sessions: {}", node.session_count))
                        .child("Advertised agents")
                        .children(example::advertised_agents(&node.host_id).map(
                            |(provider, title)| {
                                div()
                                    .text_color(rgb(colors.muted))
                                    .child(format!("{title} · {provider}"))
                            },
                        ))
                })
                .when(node.kind == Kind::Provider, |body| {
                    body.child(node.provider.clone())
                        .child(if node.advertised {
                            "Advertised by host"
                        } else {
                            "This provider is not currently advertised by the host."
                        })
                        .child(format!(
                            "{} sessions",
                            example::sessions()
                                .filter(|session| session.host_id == node.host_id
                                    && session.provider == node.provider)
                                .count()
                        ))
                        .child(
                            div()
                                .text_color(rgb(colors.muted))
                                .child("New session unavailable in example mode"),
                        )
                })
                .when(node.kind == Kind::Session, |body| {
                    body.child(format!("{} / {}", node.host, node.provider))
                        .child("Example fleet · not connected. Controls are disabled.")
                        .child(format!("Status: {}", node.status))
                        .child(session_tabs(state, cx))
                        .when(session_tab == SessionTab::Conversation, |body| {
                            body.when_some(
                                session.and_then(|session| session.approval.as_ref()),
                                |body, approval| {
                                    body.child(
                                        div()
                                            .border_y_1()
                                            .border_color(rgb(colors.separator))
                                            .py(px(12.0))
                                            .flex()
                                            .flex_col()
                                            .gap(px(8.0))
                                            .child(approval.title)
                                            .child(approval.intention)
                                            .child(
                                                div()
                                                    .id("approval-parameters")
                                                    .track_focus(approval_focus)
                                                    .focus(|style| {
                                                        style.text_color(rgb(colors.accent))
                                                    })
                                                    .child(if approval_expanded {
                                                        "⌄ Tool parameters"
                                                    } else {
                                                        "› Tool parameters"
                                                    })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.approval_expanded =
                                                            !this.approval_expanded;
                                                        cx.notify();
                                                    })),
                                            )
                                            .when(approval_expanded, |section| {
                                                section.child(
                                                    div()
                                                        .font_family("monospace")
                                                        .text_size(px(12.0))
                                                        .line_height(px(19.8))
                                                        .child(approval.parameters),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap(px(16.0))
                                                    .text_color(rgb(colors.muted))
                                                    .child(approval.approve_label)
                                                    .child(approval.deny_label),
                                            ),
                                    )
                                },
                            )
                            .child("Conversation")
                            .when_some(session, |body, session| {
                                body.child(
                                    div()
                                        .border_t_1()
                                        .border_color(rgb(colors.separator))
                                        .pt(px(12.0))
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(div().text_color(rgb(colors.muted)).child(format!(
                                            "Prompt · {}",
                                            live::last_activity(Some("2026-10-28T02:04:00.000Z"))
                                        )))
                                        .child(session.prompt)
                                        .child(response_markdown(
                                            colors,
                                            0,
                                            session.response.to_owned(),
                                        ))
                                        .when_some(session.tool, |turn, (name, status)| {
                                            turn.child(
                                                div()
                                                    .text_color(rgb(colors.muted))
                                                    .child(format!("Tool · {name} · {status}")),
                                            )
                                        })
                                        .child(
                                            div()
                                                .text_color(rgb(colors.muted))
                                                .child("complete · 420 tokens"),
                                        ),
                                )
                            })
                            .when(session.is_none(), |body| {
                                body.child(
                                    div()
                                        .text_color(rgb(colors.muted))
                                        .child("Example conversation unavailable."),
                                )
                            })
                            .when(session.is_some(), |body| {
                                body.child(
                                    div()
                                        .border_t_1()
                                        .border_color(rgb(colors.separator))
                                        .pt(px(12.0))
                                        .text_color(rgb(colors.muted))
                                        .child("Message composer unavailable in example mode."),
                                )
                            })
                        })
                        .when(session_tab == SessionTab::Files, |body| {
                            body.child("Files / changes")
                                .when_some(session, |body, session| match session.sample_file() {
                                    Some(file) => body
                                        .child("Uncommitted · ready")
                                        .child("fleet.mjs")
                                        .child(file.uri)
                                        .child(
                                            div()
                                                .id("inspect-file")
                                                .track_focus(file_focus)
                                                .focus(|style| style.text_color(rgb(colors.accent)))
                                                .text_color(rgb(colors.accent))
                                                .child(if file_preview {
                                                    "Close preview"
                                                } else {
                                                    "Inspect file"
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.file_preview = !this.file_preview;
                                                    cx.notify();
                                                })),
                                        )
                                        .when(file_preview, |body| {
                                            body.child("Before")
                                                .child(file_preview_text(
                                                    colors,
                                                    "before-file-preview",
                                                    0,
                                                    file.before,
                                                ))
                                                .child("After")
                                                .child(file_preview_text(
                                                    colors,
                                                    "after-file-preview",
                                                    0,
                                                    file.after,
                                                ))
                                        })
                                        .child(
                                            div()
                                                .text_color(rgb(colors.muted))
                                                .child("Reviewed · unavailable in example mode"),
                                        ),
                                    None => body.child(
                                        div()
                                            .text_color(rgb(colors.muted))
                                            .child("Host advertises no file changes."),
                                    ),
                                })
                        })
                        .when(session_tab == SessionTab::Info, |body| {
                            body.when_some(session, |body, session| {
                                body.child("Session")
                                    .child(session.uri)
                                    .child("Workspace")
                                    .child(session.workspace_uri())
                                    .child("Last activity")
                                    .child(live::last_activity(Some("2026-10-28T02:12:00.000Z")))
                                    .child(
                                        div()
                                            .text_color(rgb(colors.muted))
                                            .child("Dispose session · unavailable in example mode"),
                                    )
                            })
                        })
                }),
        )
}

impl Render for Relay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::Theme::global(cx).colors();
        let hosts_expanded = self.hosts_expanded;
        let sessions_expanded = self.sessions_expanded;
        let query = self.search.read(cx).value();
        let terms = example::query_terms(&query);
        let graph = example::graph_for_query(&query);
        let live_graph = self.live.then(|| live::graph(&self.live_hosts, &query));
        // The light field follows whichever graph the map is showing.
        self.refresh_lightfield(live_graph.as_ref().unwrap_or(&graph), colors, window);
        let live_sessions: Vec<_> = live::fleet_sessions(&self.live_hosts)
            .into_iter()
            .filter(|(host, session)| {
                let haystack = format!(
                    "{} {} {} {}",
                    host.label,
                    session.title,
                    session.provider,
                    session.working_directories.join(" ")
                )
                .to_lowercase();
                terms.iter().all(|term| haystack.contains(term))
            })
            .collect();
        let full_graph = example::graph();
        let selected = self.selected.as_ref().and_then(|key| {
            if self.live {
                self.live_preview
                    .as_ref()
                    .filter(|node| node.key() == *key)
                    .cloned()
            } else {
                full_graph.selected_node(key, example::host_records(), example::session_records())
            }
        });
        let detail = self.drawer.as_ref().and_then(|key| {
            full_graph.selected_node(key, example::host_records(), example::session_records())
        });
        let session_count = if self.live {
            (!self.live_hosts.is_empty()).then_some(live_sessions.len())
        } else {
            Some(example::catalog_sessions(&query).count())
        };
        // Web catalog empty state branches on the unfiltered total:
        // `sessions.length ? "No matching sessions" : "No sessions"`.
        let example_session_total = (!self.live).then(|| example::catalog_sessions("").count());
        let (live_working, live_attention, live_idle) = live::status_counts(&self.live_hosts);
        // Every legend entry carries its count, and a real zero is printed as a
        // zero rather than dropped.
        let status_label = |status: &str, label: &str| {
            let count = if self.live {
                match status {
                    "working" => live_working,
                    "attention" => live_attention,
                    _ => live_idle,
                }
            } else {
                example::sessions()
                    .filter(|session| session.status() == status)
                    .count()
            };
            format!("{count} {label}")
        };
        let active_label = status_label("working", "ACTIVE");
        let attention_label = status_label("attention", "ATTENTION");
        let idle_label = status_label("idle", "IDLE");
        let can_new_session = self.live
            && self
                .live_hosts
                .iter()
                .any(|host| host.status == "connected")
            && self.drawer.is_none()
            && !self.create_pending;
        let host_hint = if !self.live && !hosts_expanded && self.drawer.is_none() {
            let focused = example::hosts().find(|(id, _)| {
                self.host_focus
                    .get(id)
                    .is_some_and(|focus| focus.is_focused(window))
            });
            let hovered = example::hosts().find(|(id, _)| Some(*id) == self.hovered_host);
            match (hovered, focused) {
                (Some((hover_id, hover_name)), Some((focus_id, focus_name)))
                    if hover_id != focus_id =>
                {
                    Some(format!(
                        "Example · read-only · Host {hover_name} · Focus: {focus_name}"
                    ))
                }
                (Some((_, name)), _) | (_, Some((_, name))) => {
                    Some(format!("Example · read-only · Host {name}"))
                }
                _ => None,
            }
        } else {
            None
        };
        let pan = cx.listener(Self::pan_topology);
        let end_pan = cx.listener(|this, event: &MouseUpEvent, _, _| {
            if event.button == MouseButton::Left {
                this.topology_drag = None;
            }
        });
        div()
            .id("relay")
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    if this.status_open {
                        if this.live_error.is_some()
                            && this.status_close_focus.is_focused(window)
                        {
                            this.status_dismiss_focus.focus(window);
                        } else {
                            this.status_close_focus.focus(window);
                        }
                    }
                    else if this.shortcuts_open { this.shortcuts_close_focus.focus(window); }
                    else if this.live && matches!(this.drawer, Some(NodeKey::Session(_, _))) {
                        this.cycle_live_session_focus(window, event.keystroke.modifiers.shift);
                    }
                    else if this.live && matches!(this.drawer, Some(NodeKey::Provider(_, _))) {
                        if this.create_open {
                            if event.keystroke.modifiers.shift {
                                window.focus_prev();
                            } else {
                                window.focus_next();
                            }
                        } else {
                            this.drawer_close_focus.focus(window);
                        }
                    }
                    else if this.live && matches!(this.drawer, Some(NodeKey::Host(_))) {
                        let id = match &this.drawer { Some(NodeKey::Host(id)) => id, _ => unreachable!() };
                        if this.pending_host_action.is_none() && this.live_fleet.is_some()
                            && let (Some(action), Some(remove)) = (this.live_reconnect_focus.get(id), this.live_remove_focus.get(id)) {
                            let backwards = event.keystroke.modifiers.shift;
                            if this.drawer_close_focus.is_focused(window) {
                                if backwards { remove.focus(window); } else { action.focus(window); }
                            } else if action.is_focused(window) {
                                if backwards { this.drawer_close_focus.focus(window); } else { remove.focus(window); }
                            } else if remove.is_focused(window) {
                                if backwards { action.focus(window); } else { this.drawer_close_focus.focus(window); }
                            } else { this.drawer_close_focus.focus(window); }
                        } else { this.drawer_close_focus.focus(window); }
                    }
                    else {
                        let backwards = event.keystroke.modifiers.shift;
                        if backwards { window.focus_prev(); } else { window.focus_next(); }
                        if this.drawer.is_some() && this.search.read(cx).focus_handle(cx).is_focused(window) {
                            if backwards { window.focus_prev(); } else { window.focus_next(); }
                        }
                    }
                    cx.stop_propagation();
                    cx.notify();
                } else if event.keystroke.key == "escape" {
                    if this.status_open {
                        this.close_status(window);
                    } else if this.shortcuts_open {
                        this.close_shortcuts(window);
                    } else if this.drawer.is_some() {
                        this.close_drawer(window);
                    } else if this.connect_open {
                        this.close_connect(window, cx);
                    } else if this.search.read(cx).focus_handle(cx).is_focused(window) {
                        this.search.update(cx, |search, cx| search.set_value("", window, cx));
                    } else if this.hosts_expanded {
                        this.hosts_expanded = false;
                    } else if this.selected.is_some() {
                        this.selected = None;
                        if this.preview_control_focus.is_focused(window) || this.preview_close_focus.is_focused(window) {
                            this.topology_focus.focus(window);
                        }
                    } else if !this.search.read(cx).value().is_empty() {
                        let clear_focused = this.clear_search_focus.is_focused(window);
                        this.search.update(cx, |search, cx| {
                            search.set_value("", window, cx);
                            if clear_focused { search.focus(window, cx); }
                        });
                    } else {
                        return;
                    }
                    cx.stop_propagation();
                    cx.notify();
                } else if !this.shortcuts_open && !this.status_open && this.drawer.is_none() && !this.connect_open
                    && !this.search.read(cx).focus_handle(cx).is_focused(window)
                    && !event.keystroke.modifiers.control && !event.keystroke.modifiers.alt
                    && !event.keystroke.modifiers.platform
                    && (event.keystroke.key == "/" || event.keystroke.key_char.as_deref() == Some("/")) {
                    this.search.update(cx, |search, cx| search.focus(window, cx));
                    cx.stop_propagation();
                } else if !this.shortcuts_open && !this.status_open && this.drawer.is_none() && !this.connect_open
                    && !this.search.read(cx).focus_handle(cx).is_focused(window)
                    && !event.keystroke.modifiers.control && !event.keystroke.modifiers.alt
                    && !event.keystroke.modifiers.platform
                    && (event.keystroke.key == "?" || event.keystroke.key_char.as_deref() == Some("?")) {
                    this.open_shortcuts(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(colors.bg))
            .text_color(rgb(colors.text))
            .font_family(theme::Theme::global(cx).main_font.clone())
            .text_size(px(theme::Theme::global(cx).font_size))
            .child(canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            pan(event, window, cx);
                        }
                    });
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            end_pan(event, window, cx);
                        }
                    });
                },
            ).absolute().size_full())
            .child(
                div()
                    .h(px(36.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(colors.separator))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(relay_mark(colors))
                            .child(
                                div()
                                    .flex()
                                    .child("Relay")
                                    .child(div().text_color(rgb(colors.muted)).child(" / Fleet")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .when(self.live, |right| {
                                let online = self
                                    .live_hosts
                                    .iter()
                                    .filter(|host| host.status == "connected")
                                    .count();
                                right
                                    .child(
                                        div()
                                            .text_color(rgb(colors.muted))
                                            .child(format!("{online} / {} hosts online", self.live_hosts.len())),
                                    )
                                    .when(self.live_error.is_some(), |right| {
                                        right.child(
                                            div()
                                                .id("issue")
                                                .track_focus(&self.status_button_focus.clone().tab_stop(
                                                    self.drawer.is_none()
                                                        && !self.shortcuts_open
                                                        && !self.status_open,
                                                ))
                                                .focus(|style| style.text_color(rgb(colors.accent)))
                                                .child("Issue")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.open_status(window, cx);
                                                    cx.notify();
                                                })),
                                        )
                                    })
                            })
                            .child(
                                div()
                                    .id("mode")
                                    .track_focus(&self.mode_focus.clone().tab_stop(self.drawer.is_none()))
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .child(if self.live {
                                        "Show example fleet"
                                    } else {
                                        "Show live fleet"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.live = !this.live;
                                        if this.connect_open { this.close_connect(window, cx); }
                                        this.selected = None;
                                        this.focused = None;
                                        this.drawer = None;
                                        this.active_host = None;
                                        this.hovered_host = None;
                                        this.hovered_facet = None;
                                        this.topology_drag = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .child(
                        div()
                            .w(px(if hosts_expanded { 224.0 } else { 36.0 }))
                            .border_r_1()
                            .border_color(rgb(colors.separator))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .id("toggle-hosts")
                                    .track_focus(&self.hosts_toggle_focus.clone().tab_stop(self.drawer.is_none()))
                                    .focus(|style| style.text_color(rgb(colors.accent)))
                                    .h(px(36.0))
                                    .px(px(10.0))
                                    .flex()
                                    .items_center()
                                    .border_b_1()
                                    .border_color(rgb(colors.separator))
                                    .child(if hosts_expanded { "‹  Hosts" } else { "›" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.hosts_expanded = !this.hosts_expanded;
                                        cx.notify();
                                    })),
                            )
                            .when(self.live, |rail| {
                                if !hosts_expanded {
                                    return rail.child(rail_stacked_label(colors, self.live_hosts.len()));
                                }
                                rail.children(self.live_hosts.iter().map(|host| {
                                    let id = host.id.clone();
                                    let host_focus = self.live_host_focus[&id].clone();
                                    let host_id = id.clone();
                                    div().px(px(10.0)).py(px(8.0)).flex().flex_col().gap(px(4.0))
                                        .child(div().id(&host_focus).track_focus(&host_focus.clone().tab_stop(self.drawer.is_none() && !self.shortcuts_open && !self.status_open))
                                            .focus(|style| style.text_color(rgb(colors.accent)))
                                            .child(if hosts_expanded { format!("{} · {}", host.label, host.status) } else { "▤".to_owned() })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                host_focus.focus(window);
                                                this.selected = None;
                                                this.open_drawer(NodeKey::Host(host_id.clone()), host_focus.clone(), window);
                                                cx.notify();
                                            })))
                                })).when(hosts_expanded && self.live_hosts.is_empty(), |rail| rail.child(
                                    div().p(px(12.0)).text_color(rgb(colors.muted)).child("No hosts connected")
                                ))
                            })
                            .when(!self.live, |rail| {
                                if !hosts_expanded {
                                    return rail
                                        .child(rail_stacked_label(colors, example::hosts().count()));
                                }
                                rail.children(example::hosts().map(|(id, name)| {
                                    let focus = self.host_focus[id].clone();
                                    div()
                                        .id(&focus)
                                        .track_focus(&focus.clone().tab_stop(self.drawer.is_none()))
                                        .focus(|style| style.text_color(rgb(colors.accent)))
                                        .when(self.active_host.as_deref() == Some(id), |item| item.bg(rgb(colors.active)))
                                        .h(px(36.0))
                                        .px(px(10.0))
                                        .flex()
                                        .items_center()
                                        .child(if hosts_expanded { name } else { "▤" })
                                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                            if *hovered {
                                                this.hovered_host = Some(id);
                                            } else if this.hovered_host == Some(id) {
                                                this.hovered_host = None;
                                            }
                                            cx.notify();
                                        }))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            let key = NodeKey::Host(id.to_owned());
                                            focus.focus(window);
                                            this.selected = None;
                                            this.open_drawer(key, focus.clone(), window);
                                            cx.notify();
                                        }))
                                }))
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(36.0))
                                    .px(px(12.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(16.0))
                                    .border_b_1()
                                    .border_color(rgb(colors.separator))
                                    .text_color(rgb(colors.muted))
                                    .child(div().id("add-host")
                                        .track_focus(&self.connect_button_focus.clone().tab_stop(self.drawer.is_none() && !self.shortcuts_open && !self.status_open))
                                        .focus(|style| style.text_color(rgb(colors.accent)))
                                        .child("+ Add host")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if !this.live {
                                                this.focused = None;
                                                this.hovered_host = None;
                                                this.hovered_facet = None;
                                                this.topology_drag = None;
                                            }
                                            this.live = true;
                                            this.selected = None;
                                            this.drawer = None;
                                            this.connect_open = true;
                                            if this.connect_pending {
                                                this.connect_close_focus.focus(window);
                                            } else {
                                                this.connect_label.update(cx, |input, cx| input.focus(window, cx));
                                            }
                                            cx.notify();
                                        })))
                                    .child(div().flex().gap(px(16.0)).items_center()
                                        .child(active_label).child(attention_label).child(idle_label)
                                        .child(div().id("keyboard-controls")
                                            .track_focus(&self.shortcuts_button_focus.clone().tab_stop(self.drawer.is_none() && !self.shortcuts_open && !self.status_open))
                                            .focus(|style| style.text_color(rgb(colors.accent)))
                                            .child("Keys  ?")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_shortcuts(window, cx);
                                                cx.notify();
                                            })))),
                            )
                            .when(self.connect_open && self.live, |stage| stage.child(
                                div().p(px(16.0)).border_b_1().border_color(rgb(colors.separator)).flex().flex_col().gap(px(8.0))
                                    .child(div().flex().justify_between().child("Connect a host").child(
                                        div().id("close-connect").track_focus(&self.connect_close_focus)
                                            .focus(|style| style.text_color(rgb(colors.accent))).child("×")
                                            .on_click(cx.listener(|this, _, window, cx| { this.close_connect(window, cx); cx.notify(); }))
                                    ))
                                    .child("Label").child(Input::new(&self.connect_label).disabled(self.connect_pending))
                                    .child("WebSocket endpoint").child(Input::new(&self.connect_endpoint).disabled(self.connect_pending))
                                    .child("Access token").child(Input::new(&self.connect_token).disabled(self.connect_pending))
                                    .child(div().text_color(rgb(colors.muted)).child("Credentials remain in memory and are never saved."))
                                    .when(self.connect_pending, |tray| tray.child(div().text_color(rgb(colors.muted)).child("Adding host…")))
                                    .when(!self.connect_pending, |tray| tray.child(div().id("submit-connect").track_focus(&self.connect_submit_focus)
                                        .focus(|style| style.text_color(rgb(colors.accent))).child("Connect host")
                                        .on_click(cx.listener(|this, _, window, cx| this.submit_connect(window, cx)))))
                            ))
                            .when_some(self.live_error.as_ref().filter(|_| self.live), |stage, error| stage.child(
                                div().px(px(16.0)).py(px(8.0)).flex().items_center().gap(px(12.0))
                                    .child(div().text_color(rgb(colors.danger)).child(error.clone()))
                                    .child(
                                        div()
                                            .id("dismiss-banner-error")
                                            .track_focus(&self.banner_dismiss_focus.clone().tab_stop(
                                                self.drawer.is_none()
                                                    && !self.shortcuts_open
                                                    && !self.status_open,
                                            ))
                                            .focus(|style| style.text_color(rgb(colors.accent)))
                                            .child("Dismiss")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.live_error = None;
                                                cx.notify();
                                            })),
                                    )
                            ))
                            .when(self.live && self.live_fleet.is_none() && self.drawer.is_none(), |stage| stage.child(
                                div().px(px(16.0)).pb(px(8.0)).child(
                                    div()
                                        .id("restart-live-worker-stage")
                                        .track_focus(&self.restart_worker_focus.clone().tab_stop(self.drawer.is_none() && !self.shortcuts_open && !self.status_open))
                                        .focus(|style| style.text_color(rgb(colors.accent)))
                                        .child("Restart worker")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.restart_live_worker(window, cx);
                                        })),
                                )
                            ))
                            .when_some(selected.as_ref(), |stage, node| stage.child(preview(node, self, cx)))
                            .child(div().px(px(16.0)).pt(px(12.0)).flex().items_center().child(
                                div().w(px(208.0)).border_1().border_color(rgb(colors.separator)).child(
                                    Input::new(&self.search).appearance(false).bordered(false)
                                )
                            ).when(!query.is_empty(), |row| row.child(
                                div().id("clear-search").track_focus(&self.clear_search_focus.clone().tab_stop(self.drawer.is_none() && !self.shortcuts_open && !self.status_open))
                                    .px(px(10.0)).focus(|style| style.text_color(rgb(colors.accent)))
                                    .child("×").on_click(cx.listener(|this, _, window, cx| {
                                        this.search.update(cx, |search, cx| {
                                            search.set_value("", window, cx);
                                            search.focus(window, cx);
                                        });
                                        cx.notify();
                                    }))
                            ))
                            .child(div().flex_1())
                            .child(div().pr(px(16.0)).text_color(rgb(colors.muted)).child(
                                if let Some(hint) = host_hint {
                                    hint
                                } else if self.live {
                                    "Live fleet · cached data · freshness unverified".to_string()
                                } else {
                                    "Example · read-only".to_string()
                                },
                            )))
                            .child(
                                div()
                                    .id("topology-viewport")
                                    .track_focus(&self.topology_focus.clone().tab_stop(self.drawer.is_none()))
                                    .track_scroll(&self.topology_scroll)
                                    .on_key_down(cx.listener(Self::topology_key))
                                    .on_key_up(cx.listener(Self::topology_key_up))
                                    .on_mouse_down(MouseButton::Left, cx.listener(Self::start_topology_pan))
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_scroll()
                                    .when_some(live_graph.as_ref(), |stage, graph| {
                                        if graph.nodes.is_empty() {
                                            // Mirror the web empty states: no hosts means
                                            // the fleet has not started; hosts without a
                                            // match mean the query filtered everything out.
                                            let hosts_empty = self.live_hosts.is_empty();
                                            let (title, hint) = if hosts_empty {
                                                (
                                                    "Your fleet starts here",
                                                    "Connect an AHP host to map its agents and sessions.",
                                                )
                                            } else {
                                                (
                                                    "No matching nodes",
                                                    "Try another host, agent, or session name.",
                                                )
                                            };
                                            stage.child(topology_empty(colors, title, hint))
                                        } else {
                                            stage.child(div().flex().flex_col()
                                                .child(topology(graph, self, window, cx))
                                                .when(graph.total > graph.shown, |content| content.child(div().p(px(12.0)).text_color(rgb(colors.muted))
                                                    .child(format!("Showing {} of {} sessions. Open Sessions for the full list.", graph.shown, graph.total)))))
                                        }
                                    })
                                    .when(!self.live, |stage| {
                                        if graph.nodes.is_empty() {
                                            // The example fleet always advertises hosts,
                                            // so an empty map means the query matched nothing.
                                            stage.child(topology_empty(colors,
                                                "No matching nodes",
                                                "Try another host, agent, or session name.",
                                            ))
                                        } else {
                                            stage.child(
                                                div().flex().flex_col()
                                                    .child(topology(&graph, self, window, cx))
                                                    .when(graph.total > graph.shown, |content| {
                                                        content.child(div().p(px(16.0)).text_size(px(12.0))
                                                            .text_color(rgb(colors.muted))
                                                            .child(format!("Showing {} of {} sessions. Open Sessions for the full list.", graph.shown, graph.total)))
                                                    }),
                                            )
                                        }
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .border_t_1()
                                    .border_color(rgb(colors.separator))
                                    .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .id("toggle-sessions")
                                                        .track_focus(&self.sessions_toggle_focus.clone().tab_stop(self.drawer.is_none()))
                                                        .focus(|style| style.text_color(rgb(colors.accent)))
                                                        .min_h(px(56.0))
                                                        .px(px(12.0))
                                                        .child(format!("{}  Sessions {}", if sessions_expanded { "⌄" } else { "›" }, session_count.map_or_else(|| "—".to_string(), |count| if self.live { format!("{count} cached") } else { count.to_string() })))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.sessions_expanded = !this.sessions_expanded;
                                                            cx.notify();
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("new-session-catalog")
                                                        .track_focus(&self.create_submit_focus.clone().tab_stop(can_new_session && !self.shortcuts_open && !self.status_open))
                                                        .focus(|style| style.text_color(rgb(if can_new_session { colors.accent } else { colors.muted })))
                                                        .mr(px(12.0))
                                                        .child("+ New session")
                                                        .when(can_new_session, |button| {
                                                            button.on_click(cx.listener(|this, _, window, cx| {
                                                                this.open_catalog_create_session(window, cx);
                                                            }))
                                                        }),
                                                ),
                                        )
                                    .when(sessions_expanded, |catalog| {
                                        if self.live {
                                            catalog.child(
                                                div().id("live-sessions-list").max_h(px(288.0)).overflow_y_scroll()
                                                    .border_t_1().border_color(rgb(colors.separator))
                                                    .child(div().p(px(12.0)).text_color(rgb(colors.muted))
                                                        .child("Cached · freshness unverified"))
                                                    .children(live_sessions.iter().map(|(host, session)| {
                                                        let workspace = session.working_directories.first().map(String::as_str).unwrap_or("");
                                                        let key = NodeKey::Session(host.id.clone(), session.uri.clone());
                                                        let node = live::catalog_node(host, session);
                                                        let focus = self.live_session_focus[&key].clone().tab_stop(self.drawer.is_none() && !self.shortcuts_open && !self.status_open);
                                                        div().id(&focus).track_focus(&focus)
                                                            .focus(|style| style.text_color(rgb(colors.accent)))
                                                            .px(px(12.0)).py(px(8.0)).flex().items_center().justify_between().gap(px(12.0))
                                                            .border_b_1().border_color(rgb(colors.separator))
                                                            .child(div().flex().flex_col()
                                                                .child(if session.title.is_empty() { "Untitled session" } else { &session.title }.to_owned())
                                                                .child(div().text_size(px(11.0)).text_color(rgb(colors.muted))
                                                                    .child(format!("{} / {} · {}", host.label, session.provider, path_tail(workspace, if workspace.is_empty() { "—" } else { workspace })))))
                                                            .child(div().text_size(px(11.0)).text_color(rgb(colors.muted))
                                                                .child(status::session_state(Some(i64::from(session.status)), Some("ready"), host.status == "connected")))
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                this.sessions_expanded = false;
                                                                this.selected = Some(key.clone());
                                                                this.live_preview = Some(node.clone());
                                                                this.open_drawer(key.clone(), this.sessions_toggle_focus.clone(), window);
                                                                cx.notify();
                                                            }))
                                                    }))
                                                    .when(live_sessions.is_empty(), |list| list.child(
                                                        div().p(px(12.0)).text_color(rgb(colors.muted)).child("No cached matches")
                                                    )),
                                            )
                                        } else {
                                            catalog.child(
                                                div().id("sessions-list")
                                                    .max_h(px(288.0))
                                                    .overflow_y_scroll()
                                                    .border_t_1()
                                                    .border_color(rgb(colors.separator))
                                                    .children(example::catalog_sessions(&query).map(|(catalog_index, session)| {
                                                        let key = NodeKey::Session(session.host_id.to_owned(), session.uri.to_owned());
                                                        div().id(("catalog-session", catalog_index))
                                                            .track_focus(&self.catalog_focus[catalog_index].clone().tab_stop(self.drawer.is_none()))
                                                            .focus(|style| style.text_color(rgb(colors.accent)))
                                                            .px(px(12.0)).py(px(8.0))
                                                            .flex().items_center().justify_between().gap(px(12.0))
                                                            .border_b_1().border_color(rgb(colors.separator))
                                                            .child(div().flex().flex_col()
                                                                .child(if session.title.is_empty() { "Untitled session" } else { session.title })
                                                                .child(div().text_size(px(11.0)).text_color(rgb(colors.muted))
                                                                    .child(format!("{} / {} · {}", session.host_label(example::host_records()), session.provider, {
                                                                        let workspace = session.working_directories.first().copied().unwrap_or("");
                                                                        path_tail(workspace, if workspace.is_empty() { "—" } else { workspace })
                                                                    }))))
                                                            .child(div().text_size(px(11.0)).text_color(rgb(colors.muted)).child(session.status()))
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                this.sessions_expanded = false;
                                                                this.selected = Some(key.clone());
                                                                this.focused = Some(key.clone());
                                                                this.open_drawer(key.clone(), this.sessions_toggle_focus.clone(), window);
                                                                cx.notify();
                                                            }))
                                                    }))
                                                    .when(session_count == Some(0), |list| list.child(
                                                        div().p(px(12.0)).text_color(rgb(colors.muted)).child(
                                                            if example_session_total == Some(0) {
                                                                "No sessions"
                                                            } else {
                                                                "No matching sessions"
                                                            },
                                                        )
                                                    )),
                                            )
                                        }
                                    }),
                            ),
                    ),
            )
            .when_some(detail.as_ref().filter(|_| !self.live), |root, node| root.child(
                div().id("drawer-backdrop").absolute().size_full().occlude()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_drawer(window);
                        cx.notify();
                    }))
                    .child(details(node, self, cx)),
            ))
            .when(self.live && matches!(self.drawer, Some(NodeKey::Host(_))), |root| {
                let host = match &self.drawer {
                    Some(NodeKey::Host(id)) => self.live_hosts.iter().find(|host| &host.id == id),
                    _ => None,
                };
                root.child(div().id("live-drawer-backdrop").absolute().size_full().occlude()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_drawer(window);
                        cx.notify();
                    }))
                    .child(live_host_details(host, self, cx)))
            })
            .when(self.live && matches!(self.drawer, Some(NodeKey::Provider(_, _))), |root| {
                let Some(NodeKey::Provider(id, provider)) = &self.drawer else { return root; };
                let host = self.live_hosts.iter().find(|host| &host.id == id);
                root.child(div().id("live-provider-backdrop").absolute().size_full().occlude()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_drawer(window);
                        cx.notify();
                    }))
                    .child(live_provider_details(host, provider, self, cx)))
            })
            .when(self.live && matches!(self.drawer, Some(NodeKey::Session(_, _))), |root| {
                let Some(NodeKey::Session(id, uri)) = &self.drawer else { return root; };
                let host = self.live_hosts.iter().find(|host| &host.id == id);
                root.child(div().id("live-session-backdrop").absolute().size_full().occlude()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_drawer(window);
                        cx.notify();
                    }))
                    .child(live_session_details(host, uri, self, cx)))
            })
            .when(self.shortcuts_open, |root| root.child(
                div().id("shortcuts-backdrop").absolute().size_full().occlude()
                    .flex().items_center().justify_center().bg(gpui::rgba((colors.bg << 8) | 0xb3))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_shortcuts(window);
                        cx.notify();
                    }))
                    .child(div().id("shortcuts-dialog").occlude()
                        .w(px(400.0)).max_w_full().bg(rgb(colors.surface))
                        .border_1().border_color(rgb(colors.separator))
                        .child(div().h(px(56.0)).px(px(16.0)).flex().items_center().justify_between()
                            .border_b_1().border_color(rgb(colors.separator))
                            .child("Keyboard controls")
                            .child(div().id("close-shortcuts").track_focus(&self.shortcuts_close_focus)
                                .focus(|style| style.text_color(rgb(colors.accent)))
                                .child("×")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_shortcuts(window);
                                    cx.notify();
                                }))))
                        .child(div().p(px(16.0)).flex().flex_col().gap(px(12.0))
                            .child("/  ·  Focus fleet search")
                            .child("?  ·  Open keyboard controls")
                            .child("Arrow keys  ·  Move between topology nodes")
                            .child("Space  ·  Preview selected topology node")
                            .child("Enter  ·  Open selected topology node controls")
                            .child("Esc  ·  Close a dialog, clear selection or search")))
            ))
            .when(self.status_open, |root| root.child(status_dialog(self, cx)))
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", options::HELP);
        return;
    }
    let theme = match (|| {
        let options = options::Options::parse(args)?;
        let base = spec::read_bounded(&options.theme)?;
        theme::Theme::from_json(&base, "{}")
    })() {
        Ok(theme) => theme,
        Err(error) => {
            eprintln!("Relay: {error}");
            std::process::exit(2);
        }
    };
    Application::new().run(move |cx: &mut App| {
        gpui_component::init(cx);
        theme.install(cx);
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        if let Err(error) = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("relay-native".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("Relay · Fleet".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| Relay::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        ) {
            eprintln!("could not open Relay window: {error}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_cover_the_graph_and_read_the_selection_identity() {
        let graph = example::graph();
        assert_eq!(lightfield_nodes(&graph).len(), graph.nodes.len());
        assert_eq!(lightfield_edges(&graph).len(), graph.edges.len());

        let session = graph
            .nodes
            .iter()
            .find(|node| node.kind == Kind::Session)
            .expect("the example fleet has sessions");
        let session_key = session.key();
        let NodeKey::Session(host_id, uri) = session_key.clone() else {
            panic!("session nodes key as sessions");
        };
        let selection = lightfield_selection(&graph, Some(&session_key)).expect("selection");
        assert_eq!(selection.host_id, host_id);
        assert_eq!(selection.uri, Some(uri.as_str()));
        // A session selection packs host plus URI only, exactly like the web.
        assert_eq!(selection.provider, None);

        let host = graph
            .nodes
            .iter()
            .find(|node| node.kind == Kind::Host)
            .expect("the example fleet has hosts");
        let host_key = host.key();
        let selection = lightfield_selection(&graph, Some(&host_key)).expect("selection");
        assert_eq!(selection.host_id, host.host_id);
        assert_eq!(selection.uri, None);
        assert_eq!(selection.provider, None);

        assert!(lightfield_selection(&graph, None).is_none());
    }

    #[test]
    fn a_failed_read_leaves_no_pulse_baseline() {
        // Nothing stored, and a stored failure — a disconnect, a refused read
        // or a stopped worker — both leave nothing to compare against, so a
        // turn that ends across one of those gaps cannot pulse.
        assert!(pulse_baseline(None).is_none());
        assert!(pulse_baseline(Some(&Err("Host is not connected"))).is_none());
    }

    #[test]
    fn a_session_summary_change_is_what_reasks_for_the_conversation() {
        let session = |activity: Option<&str>, modified_at: &str| live::SessionView {
            uri: "ahp-session://one".to_owned(),
            title: "One".to_owned(),
            provider: "relayFixture".to_owned(),
            status: 1,
            activity: activity.map(str::to_owned),
            modified_at: modified_at.to_owned(),
            working_directories: Vec::new(),
        };
        let host = |sessions: Vec<live::SessionView>| live::HostView {
            id: "alpha".to_owned(),
            label: "Alpha".to_owned(),
            status: "connected",
            last_error: None,
            generation: 1,
            protocol_version: Some("1.0.0".to_owned()),
            agents: Vec::new(),
            sessions,
        };
        let working = vec![host(vec![session(Some("Running a turn"), "t0")])];
        let finished = vec![host(vec![session(None, "t1")])];

        assert_eq!(
            session_summary(&working, "alpha", "ahp-session://one"),
            Some((1, Some("Running a turn".to_owned()), "t0".to_owned()))
        );
        // A turn ending moves the summary, which is what re-asks for the chat.
        assert_ne!(
            session_summary(&working, "alpha", "ahp-session://one"),
            session_summary(&finished, "alpha", "ahp-session://one")
        );
        // Unchanged, unknown host and unknown session are not changes.
        assert_eq!(
            session_summary(&finished, "alpha", "ahp-session://one"),
            session_summary(&finished, "alpha", "ahp-session://one")
        );
        assert_eq!(
            session_summary(&finished, "beta", "ahp-session://one"),
            None
        );
        assert_eq!(
            session_summary(&finished, "alpha", "ahp-session://two"),
            None
        );
    }

    #[test]
    fn visible_lattice_covers_the_viewport_at_any_pan() {
        // Unpanned: the first dot is the map's own origin.
        assert_eq!(visible_lattice(0.0, 100.0, 20.0), 0..=5);
        // Panned by more than one cell on each axis: the leading dot is the one
        // before the viewport's edge and the range still reaches past the far
        // edge, so no band is left unpainted.
        assert_eq!(visible_lattice(35.0, 100.0, 20.0), 1..=7);
        assert_eq!(visible_lattice(45.0, 100.0, 20.0), 2..=8);
        assert_eq!(visible_lattice(125.0, 40.0, 20.0), 6..=9);
        // Exactly one cell, and a viewport that is not a whole number of cells.
        assert_eq!(visible_lattice(20.0, 100.0, 20.0), 1..=6);
        assert_eq!(visible_lattice(0.0, 50.0, 20.0), 0..=3);
        // The 80px majors keep the same guarantee.
        assert_eq!(visible_lattice(90.0, 200.0, 80.0), 1..=4);
    }

    #[test]
    fn pulse_needs_a_completed_turn_in_the_same_loaded_chat() {
        assert!(pulse_after_turn(
            Some("ahp-chat://one"),
            Some("turn-1"),
            Some("ahp-chat://one"),
            None,
            &["turn-0", "turn-1"],
        ));
        // The conversation failed to load on either side.
        assert!(!pulse_after_turn(
            Some("ahp-chat://one"),
            Some("turn-1"),
            None,
            None,
            &[]
        ));
        assert!(!pulse_after_turn(
            None,
            Some("turn-1"),
            Some("ahp-chat://one"),
            None,
            &["turn-1"]
        ));
        // The selection moved to another chat.
        assert!(!pulse_after_turn(
            Some("ahp-chat://one"),
            Some("turn-1"),
            Some("ahp-chat://two"),
            None,
            &["turn-1"],
        ));
        // The turn went away without the host recording it as completed.
        assert!(!pulse_after_turn(
            Some("ahp-chat://one"),
            Some("turn-1"),
            Some("ahp-chat://one"),
            None,
            &["turn-0"],
        ));
        // The turn is still running, and a chat that was merely idle never pulses.
        assert!(!pulse_after_turn(
            Some("ahp-chat://one"),
            Some("turn-1"),
            Some("ahp-chat://one"),
            Some("turn-1"),
            &[]
        ));
        assert!(!pulse_after_turn(
            Some("ahp-chat://one"),
            None,
            Some("ahp-chat://one"),
            None,
            &["turn-1"]
        ));
    }

    #[test]
    fn pulse_follows_the_completed_session_edge_and_expires() {
        let graph = example::graph();
        let edge = graph
            .edges
            .iter()
            .find(|edge| edge.session_uri.is_some())
            .expect("the example fleet has session edges");
        let signal = PulseSignal {
            host: edge.host_id.clone(),
            session: edge.session_uri.clone().expect("session edge"),
            started: Instant::now(),
        };
        let pulse = lightfield_pulse(&graph, Some(&signal)).expect("an active pulse");
        assert_eq!(
            (pulse.x1, pulse.y1, pulse.x2, pulse.y2),
            (edge.from_x, edge.from_y, edge.to_x, edge.to_y)
        );
        assert!(pulse.time < 0.1, "{pulse:?}");

        let expired = PulseSignal {
            started: Instant::now()
                .checked_sub(LIGHTFIELD_PULSE)
                .unwrap_or_else(Instant::now),
            ..signal
        };
        assert!(lightfield_pulse(&graph, Some(&expired)).is_none());

        let unknown = PulseSignal {
            host: "nobody".to_owned(),
            session: "ahp://nobody/gone".to_owned(),
            started: Instant::now(),
        };
        assert!(lightfield_pulse(&graph, Some(&unknown)).is_none());
        assert!(lightfield_pulse(&graph, None).is_none());
    }
}
