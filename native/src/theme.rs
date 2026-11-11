//! Read-only canonical theme plus validated, disposable overrides.
use crate::spec::{MAX_BYTES, object, only_keys, read_bounded};
use gpui::{App, Global, px, rgb};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Theme {
    pub palette: BTreeMap<String, u32>,
    pub main_font: String,
    pub mono_font: String,
    pub font_size: f32,
}

impl Global for Theme {}

/// Cheap immutable palette snapshot for GPUI's render closures.
#[derive(Clone, Copy, Debug)]
pub struct Colors {
    pub bg: u32,
    pub text: u32,
    pub muted: u32,
    pub secondary: u32,
    pub separator: u32,
    pub surface: u32,
    pub active: u32,
    pub accent: u32,
    pub danger: u32,
    pub warning: u32,
}

pub fn default_tokens_path() -> Result<PathBuf, String> {
    let home = match std::env::var_os("AI_HOME").filter(|v| !v.is_empty()) {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .ok_or("Set AI_HOME or pass --theme <tokens.json>")?,
        )
        .join(".ai"),
    };
    Ok(home.join("config/theme/tokens.json"))
}

fn color(value: &Value) -> Result<u32, String> {
    let text = value.as_str().ok_or("Color must be #RRGGBB")?;
    if text.len() != 7
        || !text.starts_with('#')
        || !text[1..].bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("Color must be #RRGGBB".into());
    }
    u32::from_str_radix(&text[1..], 16).map_err(|e| e.to_string())
}

fn font(value: &Value) -> Result<String, String> {
    let text = value.as_str().ok_or("Font must be a string")?;
    if text.trim().is_empty()
        || text.encode_utf16().count() > 120
        || text
            .chars()
            .any(|c| c.is_control() || "\"\\{};".contains(c))
    {
        return Err("Font must be a nonempty family name without control/CSS characters".into());
    }
    Ok(text.into())
}

fn font_size(value: &Value) -> Result<f32, String> {
    let size = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit() || c == b'.') => {
            s.parse::<f64>().ok()
        }
        _ => None,
    }
    .ok_or("main_font_size must be a number from 10 to 24")?;
    if !size.is_finite() || !(10.0..=24.0).contains(&size) {
        return Err("main_font_size must be a number from 10 to 24".into());
    }
    Ok(size as f32)
}

const REQUIRED: &[&str] = &[
    "background",
    "background_alt",
    "surface",
    "hover",
    "active",
    "border_subtle",
    "separator",
    "text",
    "text_bright",
    "text_on_accent",
    "muted",
    "muted_alt",
    "accent_1",
    "success",
    "critical",
    "warning",
];

impl Theme {
    pub fn from_json(base: &str, overrides: &str) -> Result<Self, String> {
        if base.len() > MAX_BYTES || overrides.len() > MAX_BYTES {
            return Err("Theme exceeds 100 KB".into());
        }
        let base: Value = serde_json::from_str(base).map_err(|e| e.to_string())?;
        let mut palette = object(base.get("palette").unwrap_or(&Value::Null), "palette")?.clone();
        let mut typography =
            object(base.get("typography").unwrap_or(&Value::Null), "typography")?.clone();
        let overrides: Value = serde_json::from_str(overrides).map_err(|e| e.to_string())?;
        let overrides = object(&overrides, "overrides")?;
        only_keys(overrides, &["palette", "typography"], "override")?;
        if let Some(colors) = overrides.get("palette") {
            for (key, value) in object(colors, "palette overrides")? {
                if !palette.contains_key(key) {
                    return Err(format!("Unknown palette token: {key}"));
                }
                color(value)?;
                palette.insert(key.clone(), value.clone());
            }
        }
        if let Some(fonts) = overrides.get("typography") {
            let fonts = object(fonts, "typography overrides")?;
            only_keys(
                fonts,
                &["main_font", "mono_font", "main_font_size"],
                "typography override",
            )?;
            typography.extend(fonts.clone());
        }
        for key in REQUIRED {
            if !palette.contains_key(*key) {
                return Err(format!("Missing palette token: {key}"));
            }
        }
        let palette = palette
            .iter()
            .map(|(k, v)| color(v).map(|c| (k.clone(), c)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let value = |key: &str| typography.get(key).unwrap_or(&Value::Null);
        Ok(Self {
            palette,
            main_font: font(value("main_font"))?,
            mono_font: font(value("mono_font"))?,
            font_size: font_size(value("main_font_size"))?,
        })
    }

    pub fn load(path: &Path, overrides: Option<&str>) -> Result<Self, String> {
        Self::from_json(&read_bounded(path)?, overrides.unwrap_or("{}"))
    }

    pub fn font(&self) -> gpui::Font {
        gpui::Font {
            fallbacks: Some(gpui::FontFallbacks::from_fonts(vec![
                "Noto Sans".into(),
                "Noto Sans JP".into(),
                "Segoe UI".into(),
            ])),
            ..gpui::font(self.main_font.clone())
        }
    }

    pub fn color(&self, key: &str) -> u32 {
        self.palette[key]
    }

    pub fn colors(&self) -> Colors {
        Colors {
            bg: self.color("background"),
            text: self.color("text"),
            muted: self.color("muted"),
            secondary: self.color("muted_alt"),
            separator: self.color("separator"),
            surface: self.color("surface"),
            active: self.color("active"),
            accent: self.color("accent_1"),
            danger: self.color("critical"),
            warning: self.color("warning"),
        }
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn install(&self, cx: &mut App) {
        let color = |key| rgb(self.color(key)).into();
        let t = gpui_component::Theme::global_mut(cx);
        t.mode = gpui_component::ThemeMode::Dark;
        t.background = color("background");
        t.foreground = color("text");
        t.border = color("separator");
        t.input = color("muted");
        t.muted = color("surface");
        t.muted_foreground = color("muted");
        t.primary = color("accent_1");
        t.primary_hover = color("accent_1");
        t.primary_active = color("accent_1");
        t.primary_foreground = color("text_on_accent");
        t.secondary = color("background_alt");
        t.secondary_hover = color("hover");
        t.secondary_active = color("active");
        t.secondary_foreground = color("text");
        t.accent = color("active");
        t.accent_foreground = color("text");
        t.ring = color("accent_1");
        t.caret = color("accent_1");
        t.selection = color("active");
        t.popover = color("surface");
        t.popover_foreground = color("text");
        t.scrollbar = color("background_alt");
        t.scrollbar_thumb = color("separator");
        t.scrollbar_thumb_hover = color("muted");
        t.danger = color("critical");
        t.success = color("success");
        t.warning = color("warning");
        t.font_family = self.main_font.clone().into();
        t.mono_font_family = self.mono_font.clone().into();
        t.font_size = px(self.font_size);
        t.mono_font_size = px(self.font_size);
        t.radius = px(0.0);
        t.radius_lg = px(0.0);
        t.tile_radius = px(0.0);
        t.shadow = false;
        t.tile_shadow = false;
        cx.set_global(self.clone());
    }

    /// Serializable inspection for tests and the headless --check path.
    pub fn report(&self) -> Value {
        let palette: Map<String, Value> = self
            .palette
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(format!("#{v:06x}"))))
            .collect();
        serde_json::json!({"palette":palette,"typography":{"main_font":self.main_font,"mono_font":self.mono_font,"main_font_size":self.font_size}})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> String {
        let palette: Map<String, Value> = REQUIRED
            .iter()
            .map(|k| (k.to_string(), Value::String("#000000".into())))
            .collect();
        serde_json::json!({"palette":palette,"typography":{"main_font":"Geist","mono_font":"KodeNFM","main_font_size":"13"}}).to_string()
    }

    #[test]
    fn overrides_do_not_mutate_base() {
        let base = base();
        let changed = Theme::from_json(
            &base,
            r##"{"palette":{"accent_1":"#00c4a0"},"typography":{"main_font_size":16}}"##,
        )
        .unwrap();
        assert_eq!(changed.color("accent_1"), 0x00c4a0);
        assert_eq!(changed.font_size, 16.0);
        assert_eq!(Theme::from_json(&base, "{}").unwrap().color("accent_1"), 0);
    }

    #[test]
    fn invalid_overrides_fail_closed() {
        for source in [
            r##"{"palette":{"missing":"#ffffff"}}"##,
            r##"{"palette":{"text":"red"}}"##,
            r##"{"palette":{"text":"#00000000"}}"##,
            r#"{"typography":{"main_font_size":true}}"#,
            r#"{"typography":{"main_font_size":25}}"#,
            r#"{"typography":{"main_font":"x; color:red"}}"#,
            r#"{"effects":{}}"#,
            r#"{"shape":{"radius":8}}"#,
            "null",
        ] {
            assert!(Theme::from_json(&base(), source).is_err(), "{source}");
        }
    }
}
