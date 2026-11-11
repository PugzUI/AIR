//! Bounded static json-render subset shared with the PugzUI Svelte workbench.
//! A spec has no filesystem, network, scripting, or live-fleet capabilities.
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::Path;

pub const MAX_BYTES: usize = 100_000;

#[derive(Clone, Debug)]
pub struct Spec {
    pub root: String,
    pub elements: BTreeMap<String, Element>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Stack,
    Grid,
    Panel,
    Heading,
    Text,
    Metric,
    Status,
    Button,
    Divider,
}

#[derive(Clone, Debug)]
pub struct Element {
    pub kind: Kind,
    pub props: Map<String, Value>,
    pub children: Vec<String>,
    pub notification: Option<String>,
}

impl Element {
    pub fn text(&self, key: &str) -> &str {
        self.props.get(key).and_then(Value::as_str).unwrap_or("")
    }

    pub fn boolean(&self, key: &str) -> bool {
        self.props
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn columns(&self) -> usize {
        self.props
            .get("columns")
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as usize
    }
}

pub fn read_bounded(path: &Path) -> Result<String, String> {
    read_limited(path, MAX_BYTES)
}

pub fn read_limited(path: &Path, limit: usize) -> Result<String, String> {
    if !std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .is_file()
    {
        return Err("Input must be a regular file".into());
    }
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(format!("Input exceeds {limit} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| "Input must be UTF-8".into())
}

pub fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))
}

pub fn only_keys(map: &Map<String, Value>, keys: &[&str], label: &str) -> Result<(), String> {
    if let Some(key) = map.keys().find(|key| !keys.contains(&key.as_str())) {
        return Err(format!("Unknown {label} field: {key}"));
    }
    Ok(())
}

fn identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.encode_utf16().count() > 128
        || matches!(value, "__proto__" | "constructor" | "prototype")
    {
        return Err("Element IDs must be safe, nonempty strings of at most 128 characters".into());
    }
    Ok(())
}

fn strings(props: &Map<String, Value>, required: &[&str], optional: &[&str]) -> Result<(), String> {
    for key in required {
        if !props.get(*key).is_some_and(Value::is_string) {
            return Err(format!("{key} must be a literal string"));
        }
    }
    for key in optional {
        if props.get(*key).is_some_and(|value| !value.is_string()) {
            return Err(format!("{key} must be a literal string"));
        }
    }
    Ok(())
}

fn choice(props: &Map<String, Value>, key: &str, choices: &[&str]) -> Result<(), String> {
    if let Some(value) = props.get(key)
        && !value.as_str().is_some_and(|v| choices.contains(&v))
    {
        return Err(format!("Invalid {key}"));
    }
    Ok(())
}

fn boolean(props: &Map<String, Value>, key: &str) -> Result<(), String> {
    if props.get(key).is_some_and(|value| !value.is_boolean()) {
        return Err(format!("{key} must be a boolean"));
    }
    Ok(())
}

fn component(name: &str, props: &Map<String, Value>) -> Result<Kind, String> {
    let (kind, keys): (Kind, &[&str]) = match name {
        "Stack" => (Kind::Stack, &["direction", "gap", "align"]),
        "Grid" => (Kind::Grid, &["columns", "gap"]),
        "Panel" => (Kind::Panel, &["title", "description"]),
        "Heading" => (Kind::Heading, &["text", "level"]),
        "Text" => (Kind::Text, &["content", "muted"]),
        "Metric" => (Kind::Metric, &["label", "value", "unit"]),
        "Status" => (Kind::Status, &["label", "tone"]),
        "Button" => (Kind::Button, &["label", "variant", "disabled"]),
        "Divider" => (Kind::Divider, &[]),
        _ => return Err(format!("Unknown component: {name}")),
    };
    only_keys(props, keys, name)?;
    match kind {
        Kind::Stack => {
            choice(props, "direction", &["column", "row"])?;
            choice(props, "gap", &["sm", "md", "lg"])?;
            choice(props, "align", &["start", "center", "end"])?;
        }
        Kind::Grid => {
            if !props
                .get("columns")
                .and_then(Value::as_f64)
                .is_some_and(|v| v.fract() == 0.0 && (1.0..=4.0).contains(&v))
            {
                return Err("columns must be an integer from 1 to 4".into());
            }
            choice(props, "gap", &["sm", "md"])?;
        }
        Kind::Panel => strings(props, &["title"], &["description"])?,
        Kind::Heading => {
            strings(props, &["text"], &[])?;
            if props
                .get("level")
                .is_some_and(|v| !matches!(v.as_f64(), Some(2.0 | 3.0)))
            {
                return Err("Heading level must be 2 or 3".into());
            }
        }
        Kind::Text => {
            strings(props, &["content"], &[])?;
            boolean(props, "muted")?;
        }
        Kind::Metric => strings(props, &["label", "value"], &["unit"])?,
        Kind::Status => {
            strings(props, &["label"], &[])?;
            choice(props, "tone", &["neutral", "active", "warning", "error"])?;
        }
        Kind::Button => {
            strings(props, &["label"], &[])?;
            choice(props, "variant", &["primary", "neutral"])?;
            boolean(props, "disabled")?;
        }
        Kind::Divider => {}
    }
    Ok(kind)
}

fn notification(value: &Value) -> Result<String, String> {
    let on = object(value, "on")?;
    only_keys(on, &["press"], "event")?;
    let press = object(on.get("press").unwrap_or(&Value::Null), "press")?;
    only_keys(press, &["action", "params"], "press")?;
    if press.get("action").and_then(Value::as_str) != Some("notify") {
        return Err("Only the local notify action is allowed".into());
    }
    let params = object(press.get("params").unwrap_or(&Value::Null), "params")?;
    only_keys(params, &["message"], "notify")?;
    let message = params
        .get("message")
        .and_then(Value::as_str)
        .ok_or("notify requires message")?;
    if !(1..=120).contains(&message.encode_utf16().count()) {
        return Err("notify message must contain 1..120 UTF-16 code units".into());
    }
    Ok(message.into())
}

impl Spec {
    pub fn parse(source: &str) -> Result<Self, String> {
        if source.len() > MAX_BYTES {
            return Err("Spec exceeds 100 KB".into());
        }
        let input: Value = serde_json::from_str(source).map_err(|e| e.to_string())?;
        let root_object = object(&input, "spec")?;
        only_keys(root_object, &["root", "elements", "state"], "spec")?;
        if let Some(state) = root_object.get("state")
            && !object(state, "state")?.is_empty()
        {
            return Err("Static specs do not support state bindings".into());
        }
        let root = input
            .get("root")
            .and_then(Value::as_str)
            .ok_or("Missing root")?
            .to_owned();
        identifier(&root)?;
        let raw = object(input.get("elements").unwrap_or(&Value::Null), "elements")?;
        if raw.len() > 256 {
            return Err("Spec exceeds 256 elements".into());
        }
        let mut elements = BTreeMap::new();
        for (id, raw_element) in raw {
            identifier(id)?;
            let raw_element = object(raw_element, id)?;
            only_keys(raw_element, &["type", "props", "children", "on"], id)?;
            let props = object(raw_element.get("props").unwrap_or(&Value::Null), "props")?.clone();
            let kind = component(
                raw_element
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or("Missing type")?,
                &props,
            )
            .map_err(|e| format!("{id}: {e}"))?;
            let children = raw_element
                .get("children")
                .and_then(Value::as_array)
                .ok_or("children must be an array")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or("Child reference must be a string".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !children.is_empty() && !matches!(kind, Kind::Stack | Kind::Grid | Kind::Panel) {
                return Err(format!("{id}: leaf components cannot have children"));
            }
            let notification = match raw_element.get("on") {
                Some(on) if kind == Kind::Button => Some(notification(on)?),
                Some(_) => return Err("Actions are allowed only on Button".into()),
                None => None,
            };
            elements.insert(
                id.clone(),
                Element {
                    kind,
                    props,
                    children,
                    notification,
                },
            );
        }
        let spec = Self { root, elements };
        let mut visited = HashSet::new();
        spec.visit(&spec.root, 0, false, &mut visited)?;
        if visited.len() != spec.elements.len() {
            return Err("Spec contains unreachable elements".into());
        }
        Ok(spec)
    }

    fn visit(
        &self,
        id: &str,
        depth: usize,
        inside_panel: bool,
        visited: &mut HashSet<String>,
    ) -> Result<(), String> {
        if depth > 32 {
            return Err("Spec nesting exceeds 32 levels".into());
        }
        if !visited.insert(id.to_owned()) {
            return Err(format!("Repeated or cyclic element: {id}"));
        }
        let element = self
            .elements
            .get(id)
            .ok_or_else(|| format!("Missing element: {id}"))?;
        if inside_panel && element.kind == Kind::Panel {
            return Err("Panels must not be nested".into());
        }
        for child in &element.children {
            self.visit(
                child,
                depth + 1,
                inside_panel || element.kind == Kind::Panel,
                visited,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text_spec() -> Value {
        json!({"root":"text", "elements":{"text":{"type":"Text","props":{"content":"Hello"},"children":[]}}})
    }

    #[test]
    fn literal_text_is_not_executable() {
        let mut value = text_spec();
        value["elements"]["text"]["props"]["content"] = json!("<script>alert(1)</script>");
        assert!(Spec::parse(&value.to_string()).is_ok());
    }

    #[test]
    fn rejects_unknown_and_dynamic_fields() {
        for (key, value) in [
            ("visible", json!(false)),
            ("repeat", json!({})),
            ("slots", json!({})),
            ("on", json!({})),
        ] {
            let mut spec = text_spec();
            spec["elements"]["text"][key] = value;
            assert!(Spec::parse(&spec.to_string()).is_err(), "{key}");
        }
        let mut spec = text_spec();
        spec["elements"]["text"]["props"]["content"] = json!({"$state":"/secret"});
        assert!(Spec::parse(&spec.to_string()).is_err());
    }

    #[test]
    fn validates_button_actions_and_disabled_state() {
        let spec = json!({"root":"b","elements":{"b":{"type":"Button","props":{"label":"Test","disabled":true},"children":[],"on":{"press":{"action":"notify","params":{"message":"Done"}}}}}});
        let parsed = Spec::parse(&spec.to_string()).unwrap();
        assert_eq!(parsed.elements["b"].notification.as_deref(), Some("Done"));
        assert!(parsed.elements["b"].boolean("disabled"));
        let mut bad = spec.clone();
        bad["elements"]["b"]["on"]["press"]["action"] = json!("execute");
        assert!(Spec::parse(&bad.to_string()).is_err());
    }

    #[test]
    fn rejects_amplification_and_missing_nodes() {
        let mut spec = text_spec();
        spec["root"] = json!("layout");
        spec["elements"]["layout"] = json!({"type":"Stack","props":{},"children":["text","text"]});
        assert!(Spec::parse(&spec.to_string()).is_err());
        spec["elements"]["layout"]["children"] = json!(["layout"]);
        assert!(Spec::parse(&spec.to_string()).is_err());
        spec["elements"]["layout"]["children"] = json!(["missing"]);
        assert!(Spec::parse(&spec.to_string()).is_err());
    }

    #[test]
    fn source_limit_counts_utf8_bytes() {
        assert!(
            Spec::parse(&"é".repeat(50_001))
                .unwrap_err()
                .contains("100 KB")
        );
        assert!(Spec::parse("{").is_err());
    }
}
