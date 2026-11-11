pub fn session_state(status: Option<i64>, lifecycle: Option<&str>, online: bool) -> &'static str {
    if !online {
        return "offline";
    }
    match lifecycle {
        Some("failed") => "error",
        Some("creating") => "connecting",
        _ => status_of(status),
    }
}

fn status_of(status: Option<i64>) -> &'static str {
    let Some(status) = status else {
        return "unknown";
    };
    if status & 24 == 24 {
        "attention"
    } else if status & 2 != 0 {
        "error"
    } else if status & 8 != 0 {
        "working"
    } else if status & 1 != 0 {
        "idle"
    } else {
        "unknown"
    }
}
