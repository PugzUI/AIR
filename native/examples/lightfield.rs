//! Renders the topology light field for the WebGL parity check.
//!
//! `cargo run --example lightfield -- /tmp/relay-lightfield` writes
//! `/tmp/relay-lightfield.png` and `/tmp/relay-lightfield.json`. The JSON holds
//! the exact uniforms, and `/tmp/relay-lf-harness/render.mjs` pushes them
//! through the real browser shader, so the two renders can be diffed.

use relay_native::lightfield::{self, EdgeRef, Kind, NodeRef, Palette, Selection};
use serde_json::json;
use std::fs;

const WIDTH: u32 = 1100;
const HEIGHT: u32 = 420;
const PULSE_TIME: f32 = 0.35;
const ACCENT: u32 = 0xc4ff00;

const SESSION_A1A: &str = "ahp://alpha/a1a";
const SESSION_A1B: &str = "ahp://alpha/a1b";
const SESSION_A2A: &str = "ahp://alpha/a2a";
const SESSION_B1A: &str = "ahp://beta/b1a";
const SESSION_B2A: &str = "ahp://beta/b2a";

fn nodes() -> Vec<NodeRef<'static>> {
    vec![
        NodeRef {
            kind: Kind::Host,
            x: 112.0,
            y: 24.0,
            status: "connected",
            host_id: "alpha",
            uri: None,
            provider: "",
        },
        NodeRef {
            kind: Kind::Provider,
            x: 112.0,
            y: 80.0,
            status: "connected",
            host_id: "alpha",
            uri: None,
            provider: "codex",
        },
        NodeRef {
            kind: Kind::Session,
            x: 112.0,
            y: 130.0,
            status: "working",
            host_id: "alpha",
            uri: Some(SESSION_A1A),
            provider: "codex",
        },
        NodeRef {
            kind: Kind::Session,
            x: 112.0,
            y: 170.0,
            status: "idle",
            host_id: "alpha",
            uri: Some(SESSION_A1B),
            provider: "codex",
        },
        NodeRef {
            kind: Kind::Provider,
            x: 240.0,
            y: 80.0,
            status: "connected",
            host_id: "alpha",
            uri: None,
            provider: "claude",
        },
        NodeRef {
            kind: Kind::Session,
            x: 240.0,
            y: 130.0,
            status: "attention",
            host_id: "alpha",
            uri: Some(SESSION_A2A),
            provider: "claude",
        },
        NodeRef {
            kind: Kind::Host,
            x: 456.0,
            y: 24.0,
            status: "connected",
            host_id: "beta",
            uri: None,
            provider: "",
        },
        NodeRef {
            kind: Kind::Provider,
            x: 456.0,
            y: 80.0,
            status: "connected",
            host_id: "beta",
            uri: None,
            provider: "codex",
        },
        NodeRef {
            kind: Kind::Session,
            x: 456.0,
            y: 130.0,
            status: "connected",
            host_id: "beta",
            uri: Some(SESSION_B1A),
            provider: "codex",
        },
        NodeRef {
            kind: Kind::Provider,
            x: 584.0,
            y: 80.0,
            status: "connected",
            host_id: "beta",
            uri: None,
            provider: "claude",
        },
        NodeRef {
            kind: Kind::Session,
            x: 584.0,
            y: 130.0,
            status: "offline",
            host_id: "beta",
            uri: Some(SESSION_B2A),
            provider: "claude",
        },
    ]
}

fn edge(
    from: (f32, f32),
    bend_y: f32,
    to: (f32, f32),
    host_id: &'static str,
    session_uri: Option<&'static str>,
) -> EdgeRef<'static> {
    EdgeRef {
        x1: from.0,
        y1: from.1,
        bend_y,
        x2: to.0,
        y2: to.1,
        stale: false,
        host_id,
        session_uri,
    }
}

fn edges() -> Vec<EdgeRef<'static>> {
    vec![
        edge((176.0, 56.0), 62.0, (176.0, 80.0), "alpha", None),
        edge((176.0, 56.0), 62.0, (304.0, 80.0), "alpha", None),
        edge(
            (176.0, 112.0),
            120.0,
            (176.0, 130.0),
            "alpha",
            Some(SESSION_A1A),
        ),
        edge(
            (176.0, 112.0),
            160.0,
            (176.0, 170.0),
            "alpha",
            Some(SESSION_A1B),
        ),
        edge(
            (304.0, 112.0),
            120.0,
            (304.0, 130.0),
            "alpha",
            Some(SESSION_A2A),
        ),
        edge((520.0, 56.0), 62.0, (520.0, 80.0), "beta", None),
        edge((520.0, 56.0), 62.0, (648.0, 80.0), "beta", None),
        edge(
            (520.0, 112.0),
            120.0,
            (520.0, 130.0),
            "beta",
            Some(SESSION_B1A),
        ),
        edge(
            (648.0, 112.0),
            120.0,
            (648.0, 130.0),
            "beta",
            Some(SESSION_B2A),
        ),
    ]
}

fn main() {
    let prefix = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/relay-lightfield".into());
    let palette = Palette::pugzui(ACCENT);
    let nodes = nodes();
    let edges = edges();
    let selection = Selection {
        host_id: "alpha",
        uri: Some(SESSION_A1A),
        provider: Some("codex"),
    };
    let mut geometry = lightfield::geometry(&nodes, &edges, Some(selection), 0.0, HEIGHT as f32);
    geometry.pulse = Some(lightfield::Pulse {
        x1: 176.0,
        y1: 112.0,
        x2: 176.0,
        y2: 130.0,
        time: PULSE_TIME,
    });

    let started = std::time::Instant::now();
    let pixels = lightfield::render(WIDTH, HEIGHT, 0.0, &geometry, &palette);
    let elapsed = started.elapsed();

    let rgba: Vec<u8> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
        .collect();
    let image = image::RgbaImage::from_raw(WIDTH, HEIGHT, rgba).expect("buffer matches dimensions");
    let png = format!("{prefix}.png");
    image.save(&png).expect("write png");

    let payload = json!({
        "width": WIDTH,
        "height": HEIGHT,
        "offsetY": 0.0,
        "entities": geometry.entities.iter().map(|entity| json!({
            "x": entity.x, "y": entity.y, "role": entity.role, "weight": entity.weight
        })).collect::<Vec<_>>(),
        "segments": geometry.segments.iter().map(|segment| json!([
            segment.x1, segment.y1, segment.x2, segment.y2
        ])).collect::<Vec<_>>(),
        "selected": geometry.selected.map(|selected| json!({
            "x": selected.x, "y": selected.y, "w": selected.w, "h": selected.h
        })),
        "pulse": geometry.pulse.map(|pulse| json!({
            "x1": pulse.x1, "y1": pulse.y1, "x2": pulse.x2, "y2": pulse.y2, "time": pulse.time
        })),
        "live": geometry.live,
    });
    let json_path = format!("{prefix}.json");
    fs::write(&json_path, format!("{}\n", payload)).expect("write json");

    println!(
        "{}x{} in {:?} -> {} entities, {} segments, {}",
        WIDTH,
        HEIGHT,
        elapsed,
        geometry.entities.len(),
        geometry.segments.len(),
        png
    );
}
