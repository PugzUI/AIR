//! CPU mirror of the browser topology light field.
//!
//! `ui/topology-fx.mjs` paints the neon light field with one WebGL fragment
//! shader. GPUI exposes no custom shader surface, so the same signed-distance
//! math runs here per pixel and the result is handed to GPUI as an image.
//! Rendering happens on state changes only; there is no idle animation loop.
//!
//! Every constant and expression below mirrors the shader one for one. The
//! fixture in `native/examples/lightfield.rs` renders this module to a PNG and
//! the shader harness renders the same uniforms through the real WebGL shader,
//! so the two can be diffed pixel by pixel.

/// Node housing, matching `GRAPH_CARD_SIZE` in `src/graph.mjs`.
pub const CARD_W: f32 = 128.0;
pub const CARD_H: f32 = 32.0;
/// Shader uniform budgets.
pub const MAX_ENTITIES: usize = 12;
pub const MAX_SEGMENTS: usize = 32;
/// Nodes and edges this far outside the viewport still contribute glow.
const VISIBLE_MARGIN: f32 = 48.0;
/// Distance at which the box and segment falloffs fall below one 8-bit step.
const CUTOFF: f32 = 128.0;
/// The reference grid the map draws and this field lights up, in pixels.
pub const GRID_MINOR: f32 = 20.0;
pub const GRID_MAJOR: f32 = 80.0;
/// The shader's own literal, kept verbatim so both sides agree bit for bit.
#[allow(clippy::excessive_precision)]
const HASH_SCALE: f32 = 43758.5453123;
/// The shader's own literal, kept verbatim so both sides agree bit for bit.
#[allow(clippy::approx_constant)]
const SCANLINE_FREQUENCY: f32 = 1.570796;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub const fn hex(value: u32) -> Self {
        Self::new(
            ((value >> 16) & 0xff) as f32 / 255.0,
            ((value >> 8) & 0xff) as f32 / 255.0,
            (value & 0xff) as f32 / 255.0,
        )
    }

    fn add(self, other: Self) -> Self {
        Self::new(self.r + other.r, self.g + other.g, self.b + other.b)
    }

    fn scale(self, factor: f32) -> Self {
        Self::new(self.r * factor, self.g * factor, self.b * factor)
    }

    fn mix(self, other: Self, t: f32) -> Self {
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    fn luminance(self) -> f32 {
        self.r * 0.299 + self.g * 0.587 + self.b * 0.114
    }
}

/// The PugzUI roles the shader hard-codes. `lime` follows the console accent so
/// a retinted theme carries into the light field; the rest are role constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub lime: Rgb,
    pub teal: Rgb,
    pub gold: Rgb,
    pub agent: Rgb,
}

impl Palette {
    pub const fn pugzui(lime: u32) -> Self {
        Self {
            lime: Rgb::hex(lime),
            teal: Rgb::hex(0x00c4a0),
            gold: Rgb::hex(0xc9a227),
            agent: Rgb::new(0.32, 0.72, 0.68),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Host,
    Provider,
    Session,
}

impl Kind {
    fn role(self) -> f32 {
        match self {
            Kind::Host => 0.0,
            Kind::Provider => 1.0,
            Kind::Session => 2.0,
        }
    }
}

/// One graph node in the terms the light field packs.
#[derive(Clone, Copy, Debug)]
pub struct NodeRef<'a> {
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub status: &'a str,
    pub host_id: &'a str,
    pub uri: Option<&'a str>,
    pub provider: &'a str,
}

/// One elbow route between two nodes.
#[derive(Clone, Copy, Debug)]
pub struct EdgeRef<'a> {
    pub x1: f32,
    pub y1: f32,
    pub bend_y: f32,
    pub x2: f32,
    pub y2: f32,
    pub stale: bool,
    pub host_id: &'a str,
    pub session_uri: Option<&'a str>,
}

/// The console selection, in the identity terms the packing prefers.
#[derive(Clone, Copy, Debug)]
pub struct Selection<'a> {
    pub host_id: &'a str,
    pub uri: Option<&'a str>,
    pub provider: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entity {
    pub x: f32,
    pub y: f32,
    pub role: f32,
    pub weight: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selected {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pulse {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub time: f32,
}

/// Bounded shader input: only geometry visible within the map is described.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub entities: Vec<Entity>,
    pub segments: Vec<Segment>,
    pub selected: Option<Selected>,
    pub pulse: Option<Pulse>,
    /// Carried for parity with the browser's uniform set; the fragment shader
    /// declares `u_live` but never reads it.
    pub live: bool,
}

impl Geometry {
    /// Everything that changes the rendered pixels, in one hash, so a cached
    /// image can be reused until the geometry really moves.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for entity in &self.entities {
            entity.x.to_bits().hash(&mut hasher);
            entity.y.to_bits().hash(&mut hasher);
            entity.role.to_bits().hash(&mut hasher);
            entity.weight.to_bits().hash(&mut hasher);
        }
        for segment in &self.segments {
            segment.x1.to_bits().hash(&mut hasher);
            segment.y1.to_bits().hash(&mut hasher);
            segment.x2.to_bits().hash(&mut hasher);
            segment.y2.to_bits().hash(&mut hasher);
        }
        if let Some(selected) = &self.selected {
            for value in [selected.x, selected.y, selected.w, selected.h] {
                value.to_bits().hash(&mut hasher);
            }
        }
        if let Some(pulse) = &self.pulse {
            for value in [pulse.x1, pulse.y1, pulse.x2, pulse.y2, pulse.time] {
                value.to_bits().hash(&mut hasher);
            }
        }
        self.live.hash(&mut hasher);
        hasher.finish()
    }
}

fn status_weight(status: &str) -> f32 {
    match status {
        "connected" | "working" => 1.0,
        "idle" | "attention" => 0.7,
        _ => 0.0,
    }
}

/// A session selection claims its session, then its provider, then its host.
fn matches_selection(node: &NodeRef<'_>, selection: Selection<'_>) -> bool {
    if node.host_id != selection.host_id {
        return false;
    }
    match (selection.uri, selection.provider) {
        (Some(uri), _) => node.kind == Kind::Session && node.uri == Some(uri),
        (None, Some(provider)) => node.kind == Kind::Provider && node.provider == provider,
        (None, None) => node.kind == Kind::Host,
    }
}

fn selected_node<'a, 'b>(
    nodes: &'b [NodeRef<'a>],
    selection: Selection<'_>,
) -> Option<&'b NodeRef<'a>> {
    nodes.iter().find(|node| matches_selection(node, selection))
}

pub fn geometry(
    nodes: &[NodeRef<'_>],
    edges: &[EdgeRef<'_>],
    selection: Option<Selection<'_>>,
    scroll_y: f32,
    height: f32,
) -> Geometry {
    let top = scroll_y - VISIBLE_MARGIN;
    let bottom = scroll_y + height + VISIBLE_MARGIN;
    let visible = |node: &NodeRef<'_>| node.y + CARD_H >= top && node.y <= bottom;

    let mut ordered: Vec<&NodeRef<'_>> = Vec::new();
    if let Some(selection) = selection {
        if let Some(uri) = selection.uri {
            ordered.extend(nodes.iter().filter(|node| {
                node.kind == Kind::Session
                    && node.host_id == selection.host_id
                    && node.uri == Some(uri)
                    && visible(node)
            }));
        }
        if let Some(provider) = selection.provider {
            ordered.extend(nodes.iter().filter(|node| {
                node.kind == Kind::Provider
                    && node.host_id == selection.host_id
                    && node.provider == provider
                    && visible(node)
            }));
        }
        ordered.extend(nodes.iter().filter(|node| {
            node.kind == Kind::Host && node.host_id == selection.host_id && visible(node)
        }));
    }
    ordered.extend(nodes.iter().filter(|node| visible(node)));

    let mut entities = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for node in ordered {
        if entities.len() == MAX_ENTITIES {
            break;
        }
        if !seen.insert((node.kind, node.host_id, node.uri, node.provider)) {
            continue;
        }
        entities.push(Entity {
            x: node.x + CARD_W / 2.0,
            y: node.y + CARD_H / 2.0,
            role: node.kind.role(),
            weight: status_weight(node.status),
        });
    }

    let is_selected_edge = |edge: &EdgeRef<'_>| match selection {
        Some(selection) => {
            selection.uri.is_some()
                && edge.host_id == selection.host_id
                && edge.session_uri == selection.uri
        }
        None => false,
    };
    let live: Vec<&EdgeRef<'_>> = edges.iter().filter(|edge| !edge.stale).collect();
    let mut segments = Vec::new();
    let ordered_edges = live
        .iter()
        .copied()
        .filter(|edge| is_selected_edge(edge))
        .chain(live.iter().copied().filter(|edge| !is_selected_edge(edge)));
    for edge in ordered_edges {
        if segments.len() == MAX_SEGMENTS {
            break;
        }
        let parts = [
            (edge.x1, edge.y1, edge.x1, edge.bend_y),
            (edge.x1, edge.bend_y, edge.x2, edge.bend_y),
            (edge.x2, edge.bend_y, edge.x2, edge.y2),
        ];
        for (x1, y1, x2, y2) in parts {
            if segments.len() == MAX_SEGMENTS {
                break;
            }
            if y1.max(y2) >= top && y1.min(y2) <= bottom && (x1 != x2 || y1 != y2) {
                segments.push(Segment { x1, y1, x2, y2 });
            }
        }
    }

    Geometry {
        entities,
        segments,
        selected: selection
            .and_then(|selection| selected_node(nodes, selection))
            .map(|node| Selected {
                x: node.x + CARD_W / 2.0,
                y: node.y + CARD_H / 2.0,
                w: CARD_W,
                h: CARD_H,
            }),
        pulse: None,
        live: nodes
            .iter()
            .any(|node| matches!(node.status, "connected" | "working" | "idle" | "attention")),
    }
}

pub fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let pa = (px - ax, py - ay);
    let ba = (bx - ax, by - ay);
    let h = ((pa.0 * ba.0 + pa.1 * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1).max(1.0)).clamp(0.0, 1.0);
    let dx = pa.0 - ba.0 * h;
    let dy = pa.1 - ba.1 * h;
    (dx * dx + dy * dy).sqrt()
}

pub fn sd_box_outline(px: f32, py: f32, cx: f32, cy: f32, half_w: f32, half_h: f32) -> f32 {
    let dx = (px - cx).abs() - half_w;
    let dy = (py - cy).abs() - half_h;
    let outside = (dx.max(0.0) * dx.max(0.0) + dy.max(0.0) * dy.max(0.0)).sqrt();
    (outside + dx.max(dy).min(0.0)).abs()
}

pub fn neon_falloff(distance: f32, tight_scale: f32, wide_scale: f32) -> f32 {
    let spec = 1.0 / (1.0 + distance * distance * tight_scale);
    let atmo = (-distance * wide_scale).exp();
    spec * 0.58 + atmo * 0.42
}

fn fract(value: f32) -> f32 {
    value - value.floor()
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Distance in pixels from a value to the nearest line of a lattice.
fn lattice_distance(value: f32, period: f32) -> f32 {
    let scaled = value / period;
    (scaled - scaled.round()).abs() * period
}

/// The reference grid's reactive dot: bright at a lattice point, gone by
/// `outer`. The browser tints whole lines instead; the console draws dots, so
/// the field lights the same dots the map draws.
fn grid_dot(px: f32, py: f32, period: f32, inner: f32, outer: f32) -> f32 {
    let dx = lattice_distance(px, period);
    let dy = lattice_distance(py, period);
    1.0 - smoothstep(inner, outer, (dx * dx + dy * dy).sqrt())
}

fn hash_noise(px: f32, py: f32) -> f32 {
    fract((px * 127.1 + py * 311.7).sin() * HASH_SCALE)
}

struct Prim {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

struct EntityPrim {
    entity: Entity,
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

struct Prepared<'a> {
    palette: &'a Palette,
    segments: Vec<Prim>,
    entities: Vec<EntityPrim>,
    selected: Option<Prim>,
    pulse: Option<(Prim, f32)>,
}

fn bounds(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> (f32, f32, f32, f32) {
    (
        min_x - CUTOFF,
        min_y - CUTOFF,
        max_x + CUTOFF,
        max_y + CUTOFF,
    )
}

impl<'a> Prepared<'a> {
    fn new(geometry: &Geometry, palette: &'a Palette) -> Self {
        let segments = geometry
            .segments
            .iter()
            .map(|segment| {
                let (min_x, min_y, max_x, max_y) = bounds(
                    segment.x1.min(segment.x2),
                    segment.y1.min(segment.y2),
                    segment.x1.max(segment.x2),
                    segment.y1.max(segment.y2),
                );
                Prim {
                    x1: segment.x1,
                    y1: segment.y1,
                    x2: segment.x2,
                    y2: segment.y2,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                }
            })
            .collect();
        let entities = geometry
            .entities
            .iter()
            .filter(|entity| entity.weight > 0.01)
            .map(|entity| {
                let (min_x, min_y, max_x, max_y) = bounds(
                    entity.x - CARD_W / 2.0,
                    entity.y - CARD_H / 2.0,
                    entity.x + CARD_W / 2.0,
                    entity.y + CARD_H / 2.0,
                );
                EntityPrim {
                    entity: *entity,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                }
            })
            .collect();
        let selected = geometry.selected.map(|selected| {
            let (min_x, min_y, max_x, max_y) = bounds(
                selected.x - selected.w / 2.0 - 2.0,
                selected.y - selected.h / 2.0 - 2.0,
                selected.x + selected.w / 2.0 + 2.0,
                selected.y + selected.h / 2.0 + 2.0,
            );
            Prim {
                x1: selected.x,
                y1: selected.y,
                x2: selected.w / 2.0 + 2.0,
                y2: selected.h / 2.0 + 2.0,
                min_x,
                max_x,
                min_y,
                max_y,
            }
        });
        let pulse = geometry.pulse.map(|pulse| {
            (
                Prim {
                    x1: pulse.x1,
                    y1: pulse.y1,
                    x2: pulse.x2,
                    y2: pulse.y2,
                    min_x: f32::NEG_INFINITY,
                    max_x: f32::INFINITY,
                    min_y: f32::NEG_INFINITY,
                    max_y: f32::INFINITY,
                },
                pulse.time,
            )
        });
        Self {
            palette,
            segments,
            entities,
            selected,
            pulse,
        }
    }

    fn shade(&self, px: f32, view_y: f32, p_y: f32) -> Rgb {
        let palette = self.palette;
        let minor_dot = grid_dot(px, p_y, GRID_MINOR, 0.5, 2.6);
        let major_dot = grid_dot(px, p_y, GRID_MAJOR, 0.8, 4.2);

        let mut col = Rgb::new(0.0, 0.0, 0.0);
        for segment in &self.segments {
            if px < segment.min_x
                || px > segment.max_x
                || p_y < segment.min_y
                || p_y > segment.max_y
            {
                continue;
            }
            let dx = segment.x2 - segment.x1;
            let dy = segment.y2 - segment.y1;
            if dx * dx + dy * dy <= 1.0 {
                continue;
            }
            let distance = sd_segment(px, p_y, segment.x1, segment.y1, segment.x2, segment.y2);
            let glow = neon_falloff(distance, 0.032, 0.072);
            col = col.add(palette.teal.mix(palette.lime, 0.38).scale(glow * 0.26));
        }
        for entity in &self.entities {
            if px < entity.min_x || px > entity.max_x || p_y < entity.min_y || p_y > entity.max_y {
                continue;
            }
            let distance = sd_box_outline(
                px,
                p_y,
                entity.entity.x,
                entity.entity.y,
                CARD_W / 2.0,
                CARD_H / 2.0,
            );
            let role = if entity.entity.role < 0.5 {
                palette.gold
            } else if entity.entity.role < 1.5 {
                palette.agent
            } else {
                palette.teal
            };
            col = col.add(
                role.scale(neon_falloff(distance, 0.030, 0.070) * (0.28 * entity.entity.weight)),
            );
        }
        if let Some(selected) = &self.selected
            && px >= selected.min_x
            && px <= selected.max_x
            && p_y >= selected.min_y
            && p_y <= selected.max_y
        {
            let distance =
                sd_box_outline(px, p_y, selected.x1, selected.y1, selected.x2, selected.y2);
            col = col.add(
                palette
                    .lime
                    .scale(neon_falloff(distance, 0.018, 0.050) * 0.64),
            );
        }
        if let Some((pulse, time)) = &self.pulse {
            let dx = pulse.x2 - pulse.x1;
            let dy = pulse.y2 - pulse.y1;
            let distance = sd_segment(px, p_y, pulse.x1, pulse.y1, pulse.x2, pulse.y2);
            let projection = (((px - pulse.x1) * dx + (p_y - pulse.y1) * dy)
                / (dx * dx + dy * dy).max(1.0))
            .clamp(0.0, 1.0);
            let phase = fract(projection - *time * 1.25);
            let packet = (-((phase - 0.5) * 5.0).powi(2)).exp();
            col = col.add(
                palette
                    .lime
                    .scale(neon_falloff(distance, 0.014, 0.028) * (0.28 + 0.55 * packet)),
            );
        }

        let light_mask = (col.luminance() * 2.6).clamp(0.0, 1.0);
        let grid_tint = palette.teal.mix(palette.lime, 0.58);
        col = col.add(grid_tint.scale(0.85 * light_mask * (minor_dot * 0.24 + major_dot * 0.48)));

        let scanline = 0.92 + 0.08 * (view_y * SCANLINE_FREQUENCY).sin();
        let grain = (hash_noise(px, view_y) - 0.5) * 0.022 * light_mask;
        Rgb::new(
            (col.r * scanline + grain).max(0.0),
            (col.g * scanline + grain).max(0.0),
            (col.b * scanline + grain).max(0.0),
        )
    }
}

/// Render the light field to premultiplied-opaque BGRA bytes, the layout GPUI's
/// `RenderImage` expects.
pub fn render(
    width: u32,
    height: u32,
    offset_y: f32,
    geometry: &Geometry,
    palette: &Palette,
) -> Vec<u8> {
    let width = width.max(1);
    let height = height.max(1);
    let stride = width as usize * 4;
    let mut pixels = vec![0_u8; stride * height as usize];
    let prepared = Prepared::new(geometry, palette);
    let rows = height as usize;
    let threads = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, rows);
    if threads == 1 {
        fill(&mut pixels, width, 0, offset_y, &prepared);
    } else {
        let chunk_rows = rows.div_ceil(threads);
        std::thread::scope(|scope| {
            for (index, chunk) in pixels.chunks_mut(chunk_rows * stride).enumerate() {
                let y0 = (index * chunk_rows) as u32;
                let prepared = &prepared;
                scope.spawn(move || fill(chunk, width, y0, offset_y, prepared));
            }
        });
    }
    pixels
}

fn fill(buffer: &mut [u8], width: u32, y0: u32, offset_y: f32, prepared: &Prepared<'_>) {
    let stride = width as usize * 4;
    for (index, row) in buffer.chunks_mut(stride).enumerate() {
        let view_y = y0 as f32 + index as f32 + 0.5;
        for x in 0..width {
            let px = x as f32 + 0.5;
            let color = prepared.shade(px, view_y, view_y + offset_y);
            let base = x as usize * 4;
            row[base] = (color.b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            row[base + 1] = (color.g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            row[base + 2] = (color.r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            row[base + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn node<'a>(
        kind: Kind,
        x: f32,
        y: f32,
        status: &'a str,
        host_id: &'a str,
        uri: Option<&'a str>,
        provider: &'a str,
    ) -> NodeRef<'a> {
        NodeRef {
            kind,
            x,
            y,
            status,
            host_id,
            uri,
            provider,
        }
    }

    fn edge(from: (f32, f32), bend_y: f32, to: (f32, f32), stale: bool) -> EdgeRef<'static> {
        EdgeRef {
            x1: from.0,
            y1: from.1,
            bend_y,
            x2: to.0,
            y2: to.1,
            stale,
            host_id: "alpha",
            session_uri: None,
        }
    }

    // The browser shader and this module must agree on the light field. These
    // expectations come from evaluating the GLSL expressions directly.
    #[test]
    fn reactive_grid_lights_the_same_lattice_the_map_draws() {
        // Bright on a lattice point, dark at the middle of a cell.
        assert!(grid_dot(0.0, 0.0, GRID_MINOR, 0.5, 2.6) > 0.99);
        assert!(grid_dot(GRID_MINOR, GRID_MINOR * 3.0, GRID_MINOR, 0.5, 2.6) > 0.99);
        assert!(grid_dot(GRID_MINOR / 2.0, GRID_MINOR / 2.0, GRID_MINOR, 0.5, 2.6) < 0.01);
        assert!(grid_dot(GRID_MINOR * 2.5, 0.0, GRID_MINOR, 0.5, 2.6) < 0.01);
        // The major dots are the same points, four cells apart.
        assert!(grid_dot(GRID_MAJOR, GRID_MAJOR * 2.0, GRID_MAJOR, 0.8, 4.2) > 0.99);
        assert!(grid_dot(GRID_MAJOR / 2.0, GRID_MAJOR / 2.0, GRID_MAJOR, 0.8, 4.2) < 0.01);
    }

    #[test]
    fn signed_distance_helpers_match_the_shader() {
        close(sd_segment(10.0, 20.0, 0.0, 0.0, 100.0, 0.0), 20.0);
        close(sd_segment(50.0, -13.5, 0.0, 0.0, 100.0, 0.0), 13.5);
        close(sd_segment(120.0, 7.0, 0.0, 0.0, 100.0, 0.0), 21.189_62);
        close(sd_segment(5.0, 5.0, 0.0, 0.0, 0.0, 0.0), 7.071_068);
        close(sd_segment(33.0, 44.0, 16.0, 24.0, 16.0, 120.0), 17.0);
        close(sd_box_outline(70.0, 40.0, 64.0, 16.0, 64.0, 16.0), 8.0);
        close(sd_box_outline(64.0, 16.0, 64.0, 16.0, 64.0, 16.0), 16.0);
        close(sd_box_outline(0.0, 0.0, 64.0, 16.0, 64.0, 16.0), 0.0);
        close(
            sd_box_outline(200.0, 90.0, 64.0, 16.0, 64.0, 16.0),
            92.455_39,
        );
        close(neon_falloff(0.0, 0.032, 0.072), 1.0);
        close(neon_falloff(10.0, 0.032, 0.072), 0.342_531_2);
        close(neon_falloff(40.0, 0.032, 0.072), 0.034_687_71);
        close(neon_falloff(120.0, 0.014, 0.028), 0.017_451_59);
    }

    #[test]
    fn geometry_packs_roles_weights_and_live_edges() {
        let nodes = [
            node(Kind::Host, 16.0, 24.0, "connected", "alpha", None, ""),
            node(
                Kind::Provider,
                16.0,
                80.0,
                "connected",
                "alpha",
                None,
                "codex",
            ),
            node(
                Kind::Session,
                16.0,
                130.0,
                "working",
                "alpha",
                Some("ahp://alpha/one"),
                "codex",
            ),
            node(
                Kind::Session,
                16.0,
                170.0,
                "offline",
                "alpha",
                Some("ahp://alpha/two"),
                "codex",
            ),
        ];
        let edges = [
            edge((80.0, 56.0), 62.0, (80.0, 80.0), false),
            edge((80.0, 112.0), 120.0, (80.0, 130.0), false),
            edge((80.0, 112.0), 160.0, (80.0, 170.0), true),
        ];
        let geometry = geometry(&nodes, &edges, None, 0.0, 240.0);

        assert_eq!(geometry.entities.len(), 4);
        assert_eq!(geometry.entities[0].role, 0.0);
        assert_eq!(geometry.entities[1].role, 1.0);
        assert_eq!(geometry.entities[2].role, 2.0);
        assert_eq!(geometry.entities[2].weight, 1.0);
        assert_eq!(geometry.entities[3].weight, 0.0);
        assert_eq!(
            geometry.entities[2],
            Entity {
                x: 80.0,
                y: 146.0,
                role: 2.0,
                weight: 1.0
            }
        );
        // Each vertical elbow collapses to its two straight runs, and the stale
        // edge contributes none.
        assert_eq!(geometry.segments.len(), 4);
        assert_eq!(
            geometry.segments[0],
            Segment {
                x1: 80.0,
                y1: 56.0,
                x2: 80.0,
                y2: 62.0
            }
        );
        assert_eq!(
            geometry.segments[3],
            Segment {
                x1: 80.0,
                y1: 120.0,
                x2: 80.0,
                y2: 130.0
            }
        );
        assert!(geometry.live);
    }

    #[test]
    fn geometry_keeps_only_what_the_viewport_can_show() {
        let nodes = [
            node(Kind::Host, 16.0, 24.0, "connected", "alpha", None, ""),
            node(Kind::Host, 16.0, 200.0, "connected", "beta", None, ""),
            node(Kind::Host, 16.0, 249.0, "connected", "gamma", None, ""),
        ];
        let geometry = geometry(&nodes, &[], None, 0.0, 200.0);
        // top = -48, bottom = 248: the node at 249 is out, 200 is in.
        assert_eq!(geometry.entities.len(), 2);
        assert_eq!(geometry.entities[1].y, 216.0);
    }

    #[test]
    fn geometry_prefers_the_selected_identity_and_lights_it() {
        let nodes = [
            node(Kind::Host, 16.0, 24.0, "connected", "alpha", None, ""),
            node(
                Kind::Provider,
                16.0,
                80.0,
                "connected",
                "alpha",
                None,
                "codex",
            ),
            node(
                Kind::Session,
                16.0,
                130.0,
                "working",
                "alpha",
                Some("ahp://alpha/one"),
                "codex",
            ),
            node(
                Kind::Session,
                160.0,
                130.0,
                "working",
                "alpha",
                Some("ahp://alpha/two"),
                "codex",
            ),
        ];
        let mut selected_edge = edge((80.0, 112.0), 120.0, (80.0, 130.0), false);
        selected_edge.session_uri = Some("ahp://alpha/one");
        let other = edge((224.0, 112.0), 120.0, (224.0, 130.0), false);
        let selection = Selection {
            host_id: "alpha",
            uri: Some("ahp://alpha/one"),
            provider: Some("codex"),
        };
        let geometry = geometry(&nodes, &[other, selected_edge], Some(selection), 0.0, 240.0);

        // The selected session is packed first even though it is listed later.
        assert_eq!(geometry.entities[0].x, 80.0);
        assert_eq!(
            geometry.selected,
            Some(Selected {
                x: 80.0,
                y: 146.0,
                w: CARD_W,
                h: CARD_H
            })
        );
        // The selected edge's segments come first in the uniform budget.
        assert_eq!(geometry.segments[0].x1, 80.0);
    }

    #[test]
    fn geometry_caps_entities_and_segments() {
        let hosts: Vec<String> = (0..20).map(|index| format!("host-{index}")).collect();
        let nodes: Vec<NodeRef<'_>> = hosts
            .iter()
            .map(|host| node(Kind::Host, 16.0, 24.0, "connected", host, None, ""))
            .collect();
        let edges: Vec<EdgeRef<'_>> = (0..20)
            .map(|index| edge((16.0, 56.0), 62.0, (16.0 + index as f32, 80.0), false))
            .collect();
        let geometry = geometry(&nodes, &edges, None, 0.0, 240.0);
        assert_eq!(geometry.entities.len(), MAX_ENTITIES);
        assert_eq!(geometry.segments.len(), MAX_SEGMENTS);
    }

    #[test]
    fn geometry_skips_degenerate_segments() {
        let edges = [
            edge((80.0, 56.0), 56.0, (80.0, 56.0), false),
            edge((80.0, 56.0), 62.0, (80.0, 80.0), false),
        ];
        let geometry = geometry(&[], &edges, None, 0.0, 240.0);
        // A flat elbow has no length at all, so only the second edge reaches the
        // uniform budget, as its two straight runs.
        assert_eq!(geometry.segments.len(), 2);
        assert_eq!(
            geometry.segments[0],
            Segment {
                x1: 80.0,
                y1: 56.0,
                x2: 80.0,
                y2: 62.0
            }
        );
    }

    #[test]
    fn render_leaves_untouched_pixels_black() {
        let geometry = Geometry {
            segments: vec![Segment {
                x1: 8.0,
                y1: 16.0,
                x2: 56.0,
                y2: 16.0,
            }],
            ..Geometry::default()
        };
        let palette = Palette::pugzui(0xc4ff00);
        let pixels = render(200, 200, 0.0, &geometry, &palette);
        assert_eq!(pixels.len(), 200 * 200 * 4);
        let pixel = |x: usize, y: usize| {
            let base = (y * 200 + x) * 4;
            [
                pixels[base],
                pixels[base + 1],
                pixels[base + 2],
                pixels[base + 3],
            ]
        };
        assert_eq!(pixel(190, 190), [0, 0, 0, 255]);
        let lit = pixel(32, 16);
        assert_eq!(lit[3], 255);
        // GPUI wants BGRA. The teal-lime conduit mix carries more blue than red,
        // so the low byte leads the red byte only when blue really is first.
        assert!(lit[0] > lit[2], "{lit:?}");
        assert!(lit[1] > 40, "{lit:?}");
    }

    #[test]
    fn render_is_deterministic_and_animates_only_the_pulse() {
        let geometry = Geometry {
            segments: vec![Segment {
                x1: 8.0,
                y1: 16.0,
                x2: 56.0,
                y2: 16.0,
            }],
            pulse: Some(Pulse {
                x1: 8.0,
                y1: 16.0,
                x2: 56.0,
                y2: 16.0,
                time: 0.0,
            }),
            ..Geometry::default()
        };
        let palette = Palette::pugzui(0xc4ff00);
        let first = render(64, 32, 0.0, &geometry, &palette);
        assert_eq!(first, render(64, 32, 0.0, &geometry, &palette));

        let moved = Geometry {
            pulse: geometry.pulse.map(|pulse| Pulse { time: 0.5, ..pulse }),
            ..geometry.clone()
        };
        assert_ne!(first, render(64, 32, 0.0, &moved, &palette));
    }
}
