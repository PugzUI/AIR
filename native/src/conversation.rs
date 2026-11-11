use ahp::ahp_types::state::{
    ChangesetState, ChangesetStatus, ChatInputResponseKind, ResponsePart, ToolCallState,
};

pub fn safe_error<'a>(message: &'a str, fallback: &'a str) -> String {
    let message = if message.is_empty() {
        fallback
    } else {
        message
    };
    let mut output = String::new();
    let mut rest = message;
    while let Some(index) = rest.find(['?', '&']) {
        let (prefix, candidate) = rest.split_at(index);
        output.push_str(prefix);
        if candidate.len() >= 5
            && candidate.is_char_boundary(5)
            && candidate[1..5].eq_ignore_ascii_case("tkn=")
        {
            output.push_str(&candidate[..5]);
            let value = &candidate[5..];
            // Match the browser's JavaScript whitespace rules for token redaction.
            let end = value
                .find(|ch: char| {
                    ch == '&'
                        || ch == '#'
                        || (ch.is_whitespace() && ch != '\u{0085}')
                        || ch == '\u{feff}'
                        || matches!(ch, '"' | '\'' | '<' | '>')
                })
                .unwrap_or(value.len());
            if end > 0 {
                output.push_str("[redacted]");
            }
            rest = &value[end..];
        } else {
            output.push_str(&candidate[..1]);
            rest = &candidate[1..];
        }
    }
    output.push_str(rest);
    output
}

/// Session-slot failure text: the web renders `{safeError(slotError)}. The
/// host may have restored this session under a new ID.`, where the fleet
/// pre-resolves `slot.error` to `error?.message || "Session data unavailable"`.
pub fn snapshot_error(error: &str) -> String {
    format!(
        "{}. The host may have restored this session under a new ID.",
        safe_error(error, "Session data unavailable")
    )
}

pub fn changeset_error(changes: &ChangesetState) -> Option<String> {
    (changes.status == ChangesetStatus::Error).then(|| {
        safe_error(
            changes.error.as_ref().map_or("", |error| &error.message),
            "Changeset failed to load",
        )
    })
}

pub fn response_text(part: &ResponsePart) -> Option<String> {
    match part {
        ResponsePart::Markdown(part) => Some(part.content.clone()),
        ResponsePart::ToolCall(part) => {
            let (name, fallback, status) = match &part.tool_call {
                ToolCallState::Streaming(call) => {
                    (&call.display_name, &call.tool_name, "streaming")
                }
                ToolCallState::PendingConfirmation(call) => {
                    (&call.display_name, &call.tool_name, "pending-confirmation")
                }
                ToolCallState::Running(call) => (&call.display_name, &call.tool_name, "running"),
                ToolCallState::AuthRequired(call) => {
                    (&call.display_name, &call.tool_name, "auth-required")
                }
                ToolCallState::PendingResultConfirmation(call) => (
                    &call.display_name,
                    &call.tool_name,
                    "pending-result-confirmation",
                ),
                ToolCallState::Completed(call) => {
                    (&call.display_name, &call.tool_name, "completed")
                }
                ToolCallState::Cancelled(call) => {
                    (&call.display_name, &call.tool_name, "cancelled")
                }
                ToolCallState::Unknown(_) => return None,
            };
            Some(format!(
                "Tool · {} · {status}",
                if name.is_empty() { fallback } else { name }
            ))
        }
        ResponsePart::Error(part) => Some(safe_error(&part.error.message, "Agent error")),
        ResponsePart::InputRequest(part) => Some(format!(
            "Agent requested input · {}",
            match part.response.as_ref() {
                Some(ChatInputResponseKind::Accept) => "accept",
                Some(ChatInputResponseKind::Decline) => "decline",
                Some(ChatInputResponseKind::Cancel) => "cancel",
                None => "pending",
            }
        )),
        _ => None,
    }
}
