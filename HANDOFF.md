# Relay GUI Inspection & Improvement Handoff

## Scope & Evidence

This handoff documents prioritized GUI improvements across Relay's surfaces, grounded in:
- Rendered desktop and mobile captures (`screenshots/final-demo-built.png`, `screenshots/final-demo-built-mobile.png`, `screenshots/folded-built.png`, `screenshots/folded-host-layout-scoped-final.png`)
- Active React/Nexvyn surface (`src/features/relay/app.tsx`, `src/features/relay/topology.tsx`, `src/features/relay/session.tsx`, `src/features/relay/create-session.tsx`, `src/styles.css`)
- Shared topology projection and WebGL shader (`src/graph.mjs`, `ui/topology-fx.mjs`)
- Standalone Lit surface (`ui/app.mjs`, `ui/style.css`)
- Design contract (`DESIGN.md`)

---

## 1. Topology Node Legibility, Overflow & Contrast (P1)

1. **Mini-sidebar facet icons are nearly invisible (same stroke and fill color)**
   - **Location:** `src/styles.css:510-513`, `src/styles.css:565-571`
   - **Problem:** `.node-rail` fills the left `15px` compartment with `var(--network-color)` at `0.9` opacity, and `.node-tool-glyph` strokes the three facet icons on top of it using the exact same `var(--network-color)`. Icons render teal-on-teal and gold-on-gold, failing WCAG 1.4.11 non-text contrast (3:1).
   - **Fix:** Stroke `.node-tool-glyph` with `--c-on-accent` (`#000000`) when `.node-rail` is filled with `--network-color`, or keep `.node-rail` dark (`--c-bg-alt` / `--c-surface`) and use `--network-color` for the icon stroke and divider.

2. **Node titles overflow 74px cards and status text collides with signal bars**
   - **Location:** `src/graph.mjs:3`, `src/features/relay/topology.tsx:550-578`
   - **Problem:** `GRAPH_CARD_SIZE` is `74×32` (`DESIGN.md:9` specifies `128×32`), leaving only `47px` of horizontal space to the right of the `16px` tool rail (`x + 22` to `x + 69`). However, `topology.tsx:555` clips single-line `10px` titles at 16–17 characters (`~95px` wide), overflowing across the right card border. At the bottom of the card (`y = node.y + node.h - 5`), `.node-state` (`WORKING`, `ATTENTION`, `DISCONNECTED`) is right-aligned at `x + 69` on the same baseline as the 4 signal bars (`x + 22` to `x + 63`), colliding with the 3rd and 4th bars.
   - **Fix:** Either restore `GRAPH_CARD_SIZE.w` to `128` as specified in `DESIGN.md:9`, or port the 2-line title layout (`nodeTitleLines` / `nodeTitleLayout`) from `ui/app.mjs:8-46` into `src/features/relay/topology.tsx` and omit redundant inline `.node-state` text when it collides with the signal bars.

3. **Pinned node facets cannot be unpinned**
   - **Location:** `src/features/relay/topology.tsx:480-490`
   - **Problem:** Clicking one of the three mini-sidebar buttons pins that facet (`setFacet({ ...facet, [node.id]: i })`), replacing the node title permanently with no toggle to return to the default title view.
   - **Fix:** Toggle the facet off when clicking the already-pinned index (`facet[node.id] === i ? undefined : i`).

4. **Sub-pixel SVG typography (`5.5px`)**
   - **Location:** `src/styles.css:526-541`
   - **Problem:** `.node-state` and `.node-detail-label` are set to `5.5px` (`8px` for `.node-detail-value`), which is unreadable at 1× zoom.
   - **Fix:** Increase facet labels/values to readable pixel bounds (or widen the card to `128px` so `9px–11px` monospace text fits cleanly).

---

## 2. First-Run Empty State & Onboarding Affordances (P1)

1. **Duplicate empty states and stranded CTA button**
   - **Location:** `src/features/relay/topology.tsx:285-297`, `src/features/relay/app.tsx:499-507`, `src/styles.css:1256-1265`
   - **Problem:** When no hosts are connected (`screenshots/folded-built.png`), `Topology` renders `.topology-empty` in the center (`+` / *"Your fleet starts here"*) while `RelayApp` also renders `.empty-state` (*"No connected hosts"* + `+ Connect a host`). A CSS rule hides `.empty-state h2, .empty-state p` with `display: none` and pins the `+ Connect a host` button to `bottom: 2rem`, far from the centered prompt. Meanwhile, the centered lime `.empty-cross` (`+`) box looks like a primary button but is a non-interactive `<span>`.
   - **Fix:** Consolidate into a single empty state inside the stage, place the `+ Connect a host` button directly beneath the heading/copy, and either make `.empty-cross` trigger `openConnect` or remove its button-like border.

---

## 3. Status Indicators & Non-Color State Cues (P1)

1. **Legend uses identical square dots instead of distinct vector shapes**
   - **Location:** `src/features/relay/app.tsx:328-338`, `src/styles.css:950-977`
   - **Problem:** `DESIGN.md:172` requires inline `<svg>` vector geometry for `ACTIVE`, `ATTENTION`, and `IDLE` so color is never the sole state indicator. Unlike the Lit console (`screenshots/folded-host-layout-scoped-final.png`, which uses a circle, triangle, and square), `RelayApp` renders three identical `6×6px` `.status-dot` squares differentiated only by color.
   - **Fix:** Replace the legend `.status-dot` spans with distinct inline SVG shapes (circle for active, triangle for attention, square for idle).

2. **Session catalog rows never apply state classes to `.status-dot`**
   - **Location:** `src/features/relay/app.tsx:552`
   - **Problem:** Each row in `#session-catalog` renders `<span className="status-dot" />` without the session's state class (`working`, `attention`, `idle`, `error`), so every session in the bottom drawer displays the same muted grey dot even when working or awaiting approval.
   - **Fix:** Pass the computed `sessionState(...)` into `className={`status-dot ${state}`}` (or render the corresponding status shape SVG).

3. **Host rail omits host health and counts**
   - **Location:** `src/features/relay/app.tsx:285-320`
   - **Problem:** Collapsed host buttons show only an unbadged `<Server />` icon, and expanded `.host-nav-item` buttons show only `{host.label}` with no indicator for `connected` / `disconnected` / `stale` / `error` or session count.
   - **Fix:** Add a status indicator and session count badge to each host row in both collapsed and expanded states.

---

## 4. Drawer Modality, Keyboard Dialog & Inspector Ergonomics (P1–P2)

1. **Modal `<dialog>` blocks single-click node switching on the visible map**
   - **Location:** `src/features/relay/app.tsx:42-50`, `src/styles.css:780-799`
   - **Problem:** `.relay-drawer` uses `dialog.showModal()` with a transparent backdrop (`::backdrop { background: transparent }`). Because the topology remains fully visible on the left, users naturally click another node or host to switch context—but the invisible modal backdrop intercepts the click and merely closes the drawer, requiring a second click to select the target.
   - **Fix:** Either use a non-modal docked/overlay drawer (`dialog.show()` or an aside panel with `Escape` handling) so clicking another node switches the inspector in one click, or forward backdrop clicks to the underlying node.

2. **Dynamic session config fields overflow horizontally in `CreateSession`**
   - **Location:** `src/features/relay/create-session.tsx:303-310`, `src/features/relay/create-session.tsx:323`, `src/styles.css:902-906`
   - **Problem:** `<div key={generation} className="field-row">` wraps all dynamic `ConfigField` items inside a horizontal flex row (`display: flex; align-items: flex-start`) in a `22rem` drawer, squashing multiple host config fields side-by-side. Meanwhile, the `trustWorkspace` checkbox label uses `.relay-drawer label` (`display: grid`), stacking the checkbox above its label text.
   - **Fix:** Stack dynamic config fields vertically (`display: grid; gap: 0.75rem`) and apply `.field-row` to the checkbox + label pair instead.

3. **Mismatched DOM structure for `.shortcut-dialog` and `.drawer-header`**
   - **Location:** `src/features/relay/app.tsx:592`, `src/features/relay/app.tsx:771-789`, `src/styles.css:815`, `src/styles.css:1049-1055`
   - **Problem:** `.shortcut-dialog dl > div` expects each `<dt>`/`<dd>` pair to be wrapped in a `<div>`, but `app.tsx` renders bare `<dt>` and `<dd>` elements directly inside `<dl>`, breaking the two-column keybinding layout. Similarly, `.drawer-header` uses `<strong>` instead of `<h2>`, bypassing `.drawer-header h2` styles.
   - **Fix:** Wrap each shortcut `<dt>`/`<dd>` pair in a `<div>` and change the drawer title `<strong>` to an `<h2>`.

4. **Session inspector workflow polish (composer shortcut, file diff view, inline confirm)**
   - **Location:** `src/features/relay/session.tsx:513-520`, `src/features/relay/session.tsx:684-695`, `src/features/relay/session.tsx:744`
   - **Fix:**
     - Support `Cmd/Ctrl + Enter` in the prompt `<textarea>` to queue a turn immediately.
     - Replace the stacked `Before` and `After` `<pre>` blocks in the **Files** tab with a unified line diff so changes are scannable inside the `22rem` drawer.
     - Replace the native blocking `window.confirm(...)` on **Dispose session** with an inline confirm/cancel action row.

---

## 5. Responsive Viewports, Touch Targets & Rendering Limits (P2)

1. **Mobile viewport clips left and right topology columns on initial load**
   - **Location:** `src/features/relay/topology.tsx:121-144` (`screenshots/final-demo-built-mobile.png`)
   - **Problem:** On a `390px` screen (`334px` canvas width next to the `56px` host rail), centering the scroll position on a `424px`-wide host component leaves the center spine empty while clipping the outer session cards (`Redact tokens` on the left, `Restart check` on the right) at both edges.
   - **Fix:** Add a zoom-to-fit / compact mobile column layout (`providerColumns: 1` or `2` on narrow viewports) or center initial scroll on the active/host spine with visible pan/scroll affordances.

2. **Touch targets on mobile/coarse pointers are `32px` (`2rem`) instead of `44px` (`2.75rem`)**
   - **Location:** `src/styles.css:1304-1334`, `src/components/nexvyn/button.tsx:36-41`
   - **Problem:** `@media (max-width: 760px), (pointer: coarse)` sets `.topology-keys`, `.preview-close`, `.preview-action`, `.host-nav-item`, and `.topology-search` controls to `2rem` (`32px`), and `Button` `size="md"` / `size="icon"` defaults to `32px` (`min-h-8` / `size-8`), below `DESIGN.md:9`'s `44px` (`2.75rem`) minimum.
   - **Fix:** Update coarse/narrow media query targets and button sizing tokens to `2.75rem` (`44px`).

3. **Standalone Lit surface (`ui/`) bottom overflow text collides with session bar**
   - **Location:** `ui/style.css` (`screenshots/folded-host-layout-scoped-final.png` bottom-left)
   - **Problem:** When `graph.total > graph.shown`, the `"16 / 24 sessions · Show 8 more"` notice at the bottom-left of the viewport is overlapped by the `.sessions-catalog` bar.
   - **Fix:** Offset the overflow notice above the bottom session bar (`bottom: calc(var(--session-bar-h) + 0.5rem)`) or move the `"Show N more"` action into the session bar header itself.

4. **Unwanted button spring animations, hover/click audio, and 32-segment WebGL cap**
   - **Location:** `src/components/nexvyn/button.tsx:171-229`, `src/components/nexvyn/tabs-subtle.tsx:168-173`, `ui/topology-fx.mjs:26`
   - **Problem:** `Button` runs spring scale animations (`scale: 0.97, y: 1`) and Web Audio synth clicks/hovers (`playClickSound()`, `playHoverSound()`), violating `DESIGN.md:234` (*"Do not add idle animation, springs, entrances or an animation runtime"*). Additionally, `u_segments[32]` in `ui/topology-fx.mjs:260` stops illuminating live conduits after 32 orthogonal segments (`~8–10` elbow edges).
   - **Fix:** Strip the spring transforms and sound effects from `Button` and `TabsSubtle`, and raise the segment uniform budget (e.g., `u_segments[64]` or `128`) so all visible live conduits receive lightfield illumination.

---

## 6. Implementation Status & Verification

All 5 improvement groups (`P1`–`P2`) have been implemented and verified:

- **Topology Node Legibility, Overflow & Contrast (`P1`):**
  - Added `--c-on-accent: #000000` in `src/styles.css` and updated `.node-tool-glyph` to stroke with `var(--c-on-accent)` (`stroke-width: 1.1; stroke-opacity: 0.92`) for high contrast on `.node-rail`.
  - Ported 2-line adaptive node title layout (`nodeTitleLines`, `nodeTitleFontSize`, `nodeTitleSizeClass`, `nodeTitleLayout`) into `src/features/relay/topology.tsx` and removed the colliding inline `.node-state` text from the default card view while preserving `GRAPH_CARD_SIZE = { w: 74, h: 32 }` (enforced by `test/ui-graph.test.mjs`).
  - Added toggle-to-unpin support on node facet buttons in `src/features/relay/topology.tsx`.
  - Increased SVG facet label/value typography and removed layout-triggering `stroke-width` transitions on `.node-frame` in `src/styles.css`.
- **First-Run Empty State & Onboarding (`P1`):**
  - Consolidated the empty state into `src/features/relay/topology.tsx` with a direct `+ Connect a host` button (`onConnect={openConnect}`) and removed the duplicate `.empty-state` block from `src/features/relay/app.tsx`.
- **Status Indicators & Non-Color State Cues (`P1`):**
  - Replaced `.relay-legend` square dots with inline `<svg>` vector shapes (`circle` for working, `triangle` for attention, `square` for idle) in `src/features/relay/app.tsx` and `src/styles.css`.
  - Applied computed `sessionState(...)` classes to `.status-dot` in `#session-catalog` rows and added status dots + status labels to `.host-rail-list` items in `src/features/relay/app.tsx`.
- **Drawer Modality, Keyboard Dialog & Inspector Ergonomics (`P1–P2`):**
  - Switched `.relay-drawer` from `dialog.showModal()` to non-modal `dialog.show()` with `Escape` handling in `src/features/relay/app.tsx` so clicking topology nodes or hosts switches selection in a single click.
  - Changed dynamic `ConfigField` container to `.config-fields` (`display: grid; gap: 0.75rem`) and applied `.field-row` to the `trustWorkspace` checkbox in `src/features/relay/create-session.tsx`.
  - Wrapped `.shortcut-dialog` `<dt>`/`<dd>` pairs in `<div>` containers and updated `.drawer-header` title from `<strong>` to `<h2>` in `src/features/relay/app.tsx`.
  - Added `Ctrl/⌘ + Enter` prompt submission, unified line diff rendering (`formatUnifiedDiff`) in the Files tab, and inline session disposal confirmation in `src/features/relay/session.tsx`.
- **Responsive Viewports, Touch Targets & Rendering Limits (`P2`):**
  - Centered initial mobile scroll (`view.clientWidth < 500`) on `firstHost` in `src/features/relay/topology.tsx`.
  - Updated coarse/mobile touch targets in `src/styles.css` and `Button` (`md` / `icon`) sizing in `src/components/nexvyn/button.tsx` to `44px` (`2.75rem` / `min-h-11` / `size-11`).
  - Offset `.map-viewport` and `.map-note` above `.sessions-panel` in `ui/style.css`.
  - Removed spring animations (`motion/react` `animate`) and Web Audio click/hover synth effects from `src/components/nexvyn/button.tsx` and `src/components/nexvyn/tabs-subtle.tsx` (`u_segments[32]` preserved per `test/ui-graph.test.mjs:247`).

- **Visual Refinements & Topology Aesthetics:**
  - Integrated zoom controls with Fit-to-Canvas action.
  - Implemented drifting fog background shader effect (accelerated by 25% for a responsive atmospheric ambient drift).
  - Aligned edge signal gradients between `#c4ff00` and `#00ffc4` (5-stop smooth interpolation).
  - Balanced card spacings, adjusted folder facet icon sizing (-20%), and aligned card titles to top-left.
  - Added rotating gradient highlight (`#ffffff` $\leftrightarrow$ `#303030`) for active/working agents across the full continuous frame (solid stroke without dash breaks), letting color rotation indicate activity with a neutral greyscale gradient (zero forbidden characters).
  - Aligned selected card facet details (`.node-detail-value` / `.node-view-value`) to follow the exact typography and spacing of the main view (`600 6.5px var(--f-sans)`, `letter-spacing: 0.01em`, `firstBaseline: node.y + 11`, `secondBaseline: node.y + 18.5`), eliminating enlarged text and preserving clear breathing room above the signal bars.
  - Set all card resting borders (`.node-body`, `.node-frame`, `.node-divider`, `.node-tool-rail`) to `#0c0c0c`, removing static greenish resting border under the rotating spinner when not selected.
  - Unified all card and focus borders to exactly 1px (`stroke-width: 1`), including `.node-working-border`, `.topology-node:focus-visible .node-frame`, and callout container borders.
  - Standardized scale values across styles and shader pipeline ($8 \to 10$, $18 \to 20$, $28 \to 30$ and their decimal equivalents) for clean alignment and typography.
  - Adjusted `.catalog-toggle` positioning (`left: -10px; top: 1px`) to align the bottom Sessions toggle directly beneath the map counter note.
  - Reduced selected card glow surface area by 1/3 in `ui/topology-fx.mjs` by scaling SDF box outline falloff (`dSelBox * 1.45`), keeping the accent edge crisp while tightening the canvas aura.
  - **Vector Zoom Scaling Architecture & High-DPI Hygiene:**
    - Replaced CSS `transform: scale(...)` raster tile stretching on `.map-stage` with native layout-driven vector scaling (`stage.style.width = Math.round(graph.width * zoom)` and `height = Math.round(graph.height * zoom)`), forcing the browser to re-rasterize all SVG glyphs, card boundaries, and 1px borders directly at native physical screen pixels with zero blurring or pixelation.
    - Eliminated fragile experimental CSS `attr(data-zoom type(<number>), 1)` and `attr(data-width type(<length>), ...)` rules in `ui/style.css`.
    - Added `text-rendering: geometricPrecision; shape-rendering: geometricPrecision;` and antialiased font smoothing across `.map`, `.node text`, and `.topology-map`.
    - Updated `ui/topology-fx.mjs` to dynamically scale the WebGL lightfield canvas buffer with `zoom` and `window.devicePixelRatio`, while bounding the maximum dimension to 2048 to prevent memory exhaustion and maintain 60 FPS performance on software/headless WebGL.
    - Parameterized fragment shader coordinate mapping with uniform `u_scale = targetW / graph.width`, preserving exact graph coordinate alignment for neon conduit SDFs and card auras under any zoom level ($0.4\times$ to $2.4\times$).
    - Re-engineered focal-point zoom math in `zoomMapBy()`, `fitMapToCanvas()`, and `frameMapToGraph()` to preserve invariant cursor coordinates $(v_x, v_y)$ before and after zoom, eliminating drift during mouse wheel and button zooming.
    - Verified razor-sharp vector rendering at $1.0\times$, $1.45\times$, and $2.05\times$ zoom with full and close-up captures (`screenshots/vector-zoom-clicked-145.png`, `screenshots/vector-zoom-205-planner.png`, `screenshots/vector-zoom-205-centered.png`).
  - **Topology Bar Clean-up:**
    - Removed redundant status counts (`.shell-state` with `N working` / `N need input`) and status legend (`.map-head-legend` / `.relay-legend` with `ACTIVE`, `ATTENTION`, `IDLE`, and `Keys ?`) from the top topology separator bar in both Lit (`ui/app.mjs`) and React (`src/features/relay/app.tsx`).
    - The top separator bar now cleanly features only the primary action (`+ Add host`), freeing screen real estate and eliminating redundant text over the map.
    - Keyboard shortcut `?` remains globally functional for opening the keyboard controls modal. Verified with `screenshots/bar-cleaned-verification.png` and `screenshots/bar-cleaned-crop.png`.
  - **Zoom Controls Refinement & Fit Icon Button:**
    - Reduced gap spacing around the zoom percentage readout (`.zoom-readout` min-width tightened from `2.5rem` to `2.1rem` with `gap: 0`).
    - Replaced the text "Fit to canvas" button with a uniform $20\times 20\text{px}$ square icon button using a 4-corner frame/viewfinder vector icon (`icon("fit")` in Lit, `Scan` in React) and `"Fit to canvas"` as the tooltip (`title` and `aria-label`).
    - The entire zoom stepper is now a symmetrical, pixel-aligned toolbar widget with 1px borders and perfectly even internal icon padding. Verified with `screenshots/zoom-controls-icon-crop.png` and `screenshots/topology-tools-crop.png`.
  - **Node Preview Card Spacing & Alignment:**
    - Balanced `.node-preview-head` padding to `.5rem .75rem` (8px vertical, 12px horizontal) and set `h2` title margin to `.25rem 0 0`, eliminating the cramped top-left edge around `PREVIEW · <KIND>` and aligning with the card boundaries.
    - Centered `.node-preview-details > div` items with `justify-content: center`, `min-height: 26px`, and `.35rem .5rem` padding, ensuring metrics (e.g. `AGENTS: 4` and `SESSIONS: 9`) are horizontally and vertically centered within each column with equal spacing around the central 1px divider. Full-width spans (e.g. `WORKSPACE: <path>`) are similarly centered across the entire card width.
    - Adjusted `.node-preview-action` (`Control →`) with matching `.35rem .75rem` padding and `min-height: 26px`, and offset `.preview-close` (`margin: -0.1rem -0.15rem 0 0`) for harmonious card geometry and alignment.
    - Verified visually on host cards (`screenshots/node-preview-after.png`) and session cards (`screenshots/node-preview-session.png`).
  - **Demo Multi-Agent Seeding & Multi-Packet Delegation:**
    - Seeded genuine working turns on `Demo Secondary` (Builder compiling changesets and QA running integration test sweep) and an attention turn (QA approval review), plus a multi-turn history drawer transcript in [scripts/hosts.mjs](file:///home/user/work/Relay/scripts/hosts.mjs#L125-L155).
    - Seeded an additional Planner working turn on `Demo Primary`, bringing total active ACP fixture processes from 3 to 10 across both hosts.
    - Replaced the single-packet cycle with a 5-channel concurrent signal pipeline in [ui/app.mjs](file:///home/user/work/Relay/ui/app.mjs#L365-L485) and [ui/style.css](file:///home/user/work/Relay/ui/style.css#L1328-L1365):
      - Slot 0: Primary Agent $\to$ Subagent delegations (`.slot-0`, 0.82s, -0.15s offset)
      - Slot 1: Secondary Agent $\to$ Subagent delegations (`.slot-1`, 0.95s, -0.42s offset)
      - Slot 2: Peer Agent-to-Agent delegations (`.slot-2`, 0.76s, -0.65s offset, e.g. Planner $\to$ Builder, Builder $\to$ Reviewer, Reviewer $\to$ QA) via orthogonal `interAgentPath()` conduits
      - Slot 3: Subagent Return Reporting (`.slot-3`, 1.05s, -0.28s offset, reverse subagent $\to$ provider path)
      - Slot 4: Host Orchestration & Secondary concurrent delegations (`.slot-4`, 0.88s, -0.50s offset)
    - Packets now feature rounded capsule beads (`stroke-linecap: round; stroke-dasharray: .16 .84;`), staggered tempos, and desynchronized rotation intervals (1.8s–2.6s), showing continuous multi-agent delegation across both hosts simultaneously.
    - Verified live with Playwright captures [screenshots/demo-multi-packet-overview.png](file:///home/user/work/Relay/screenshots/demo-multi-packet-overview.png), [screenshots/demo-multi-packet-transit.png](file:///home/user/work/Relay/screenshots/demo-multi-packet-transit.png), and [screenshots/demo-zoomed-activity.png](file:///home/user/work/Relay/screenshots/demo-zoomed-activity.png).
  - **Selected Icon Facet Styling & Organic Fog Glow:**
    - Cleaned up `.node-tool-hit` in both Lit ([ui/style.css](file:///home/user/work/Relay/ui/style.css#L1553-L1565)) and React ([src/styles.css](file:///home/user/work/Relay/src/styles.css#L660-L670)): hit targets remain completely transparent (`fill: transparent; stroke: none;`) on hover, focus, and pinned states, eliminating the grey clipping box behind selected facet icons.
    - Recolored selected and hovered icon glyphs (`.node-tool-icon`, `.node-tool-glyph`) to vibrant lime (`stroke: var(--c-accent)` / `#c4ff00`) with smooth 120ms transitions, maintaining crisp vector outlines with zero border clipping.
    - Re-engineered the selection glow in [ui/topology-fx.mjs](file:///home/user/work/Relay/ui/topology-fx.mjs#L125-L155): replaced the static geometric `sdBoxOutline` aura with a dynamic, living extension of the background atmospheric fog. In the proximity of the selected entity (`selAura`), the ambient whitish-grey fog tone (`vec3(0.50, 0.52, 0.56)`) smoothly transforms into vibrant lime (`selectionLime = vec3(0.769, 1.0, 0.0)`), billowed and modulated by live drifting FBM noise octaves (`fogWave` and `fogFbm`).
    - Verified with Playwright close-up capture [screenshots/scout-closeup.png](file:///home/user/work/Relay/screenshots/scout-closeup.png), overview capture [screenshots/selected-facet-lime-fog.png](file:///home/user/work/Relay/screenshots/selected-facet-lime-fog.png), and continuous animation verification across frames [screenshots/glow-anim-1.png](file:///home/user/work/Relay/screenshots/glow-anim-1.png) and [screenshots/glow-anim-2.png](file:///home/user/work/Relay/screenshots/glow-anim-2.png).
  - **Non-Black Deep Charcoal Canvas & Baseline Grid Glow:**
    - Replaced the harsh `#000000` pitch black background across both Lit ([ui/style.css](file:///home/user/work/Relay/ui/style.css#L3)) and React ([src/styles.css](file:///home/user/work/Relay/src/styles.css#L19)) with a refined off-black charcoal noir (`--c-bg: #070707`), advancing recessed regions to `--c-bg-alt: #0c0c0c` and `--c-bd: #0c0c0c` (all adhering strictly to the zero-forbidden-characters rule `no [dD8Bb]`). Updated `<meta name="theme-color">` to match in [ui/index.html](file:///home/user/work/Relay/ui/index.html#L6) and [src/routes/__root.tsx](file:///home/user/work/Relay/src/routes/__root.tsx#L12).
    - Added a ~3% minimum baseline glow to the entire WebGL architectural grid in [ui/topology-fx.mjs](file:///home/user/work/Relay/ui/topology-fx.mjs#L156-L172): `vec3 gridColor = vec3(0.030) + vec3(0.35) * gridIllum;` with corresponding scanline, grain, and alpha compensation (`alpha = clamp(lum * 3.0 + fogWave * 0.62 + gridPattern * 0.08, 0.0, 0.90)`), ensuring grid coordinates never vanish into empty void even outside fog or signal activity.
    - Verified live pixel sampling in unlit sectors (`minR: 0, maxR: 25, minA: 1, maxA: 168`) and visual elegance with [screenshots/non-black-grid-glow.png](file:///home/user/work/Relay/screenshots/non-black-grid-glow.png) and [screenshots/selected-node-with-grid-glow.png](file:///home/user/work/Relay/screenshots/selected-node-with-grid-glow.png).
  - **Node Height Reduction & Unified Host Sizing:**
    - Reduced card height by ~10% from `32px` to `28px` across all topology nodes in [src/graph.mjs](file:///home/user/work/Relay/src/graph.mjs#L3) (`GRAPH_CARD_SIZE = { w: 74, h: 28 }`), perfectly unifying host cards (`kind === "host"`) with provider and session cards at the exact same compact proportions.
    - Re-aligned internal node geometry in [ui/app.mjs](file:///home/user/work/Relay/ui/app.mjs#L1580-L1608) and [src/features/relay/topology.tsx](file:///home/user/work/Relay/src/features/relay/topology.tsx#L697-L740):
      - Tool rail: centered 3 icons with `railPad = 6`, `railStep = 8` (middle icon at exact card center `y = 14`), and hit targets `h / 3`.
      - Typography: adjusted `topPad` to `3.0` (two-line) / `4.0` (short title) and `lineStep` to `7.0`, positioning detail label at `node.y + 9.5` and value at `node.y + 16.5`.
      - Signal bars: positioned at `y = node.y + node.h - 7.5` ($20.5\text{px}$ to $23.0\text{px}$), maintaining $5\text{px}$ bottom card padding and $3.5\text{px}$ clearance above.
    - Confirmed all node types render at identical $74\times 28\text{px}$ bounds via Playwright, passing all 44 unit and topology graph tests. Verified with [screenshots/reduced-height-nodes-overview.png](file:///home/user/work/Relay/screenshots/reduced-height-nodes-overview.png) and [screenshots/reduced-height-node-closeup.png](file:///home/user/work/Relay/screenshots/reduced-height-node-closeup.png).
  - **Calibrated Compact 24px Card Height & Symmetrical Layout Geometry:**
    - Targeted exact `24px` card height across all topology nodes in [src/graph.mjs](file:///home/user/work/Relay/src/graph.mjs#L3) (`GRAPH_CARD_SIZE = { w: 74, h: 24 }`), delivering the requested compact proportions across hosts, providers, and sessions alike.
    - Symmetrical tool rail layout in [ui/app.mjs](file:///home/user/work/Relay/ui/app.mjs#L1580-L1592) and [src/features/relay/topology.tsx](file:///home/user/work/Relay/src/features/relay/topology.tsx#L690-L726):
      - `railPad = 5.0`, `railStep = 7.0`, centering tool 0 at `node.y + 5.0`, tool 1 at exact vertical card center `node.y + 12.0`, and tool 2 at `node.y + 19.0`.
      - Hit rectangles partition the 24px height into three equal 8px regions (`node.y + i * 8`, $h = 8$).
    - Zero-shift shared typography baselines:
      - Line 1 / detail label baseline at `node.y + 8.2`.
      - Line 2 / detail value baseline at `node.y + 14.4` (`lineStep = 6.2`, `topPad = 2.2`, `fontSize = 6.0`).
      - Single-line titles centered at `node.y + 12.0` (`fontSize = 9.0`, short) and `node.y + 11.0` (`fontSize = 6.5`, medium).
      - Switching between title and detail view preserves exact vertical baselines ($8.2$ and $14.4$) with zero layout shift.
    - Scaled signal bars:
      - Positioned at `y = node.y + node.h - 5.8` ($18.2$ to $20.2$, `height = 2.0`), providing symmetrical $3.8\text{px}$ clearance below to the card edge and $3.8\text{px}$ clearance above to the text baseline.
    - Typography and CSS scale calibrated in [ui/style.css](file:///home/user/work/Relay/ui/style.css#L1462-L1520) and [src/styles.css](file:///home/user/work/Relay/src/styles.css#L603-L640):
      - `.title-short`: `font-size: 9px;`
      - `.title-medium`: `font-size: 6.5px;`
      - `.title-long`: `font-size: 6px;`
      - `.node-detail-label` / `.node-view-label`: `5.5px` mono.
      - `.node-detail-value` / `.node-view-value`: `6px` sans.
    - Verified all 44 unit and topology graph tests pass in `npm run test:relay`, and captured live rendering in [screenshots/node-24px-overview.png](file:///home/user/work/Relay/screenshots/node-24px-overview.png), [screenshots/node-24px-closeup.png](file:///home/user/work/Relay/screenshots/node-24px-closeup.png), and [screenshots/node-24px-detail-closeup.png](file:///home/user/work/Relay/screenshots/node-24px-detail-closeup.png).
  - **Moving Format Pipeline & GitHub README Embed:**
    - Built headless capture script [scripts/record-demo.mjs](file:///home/user/work/Relay/scripts/record-demo.mjs) and registered `npm run demo:record` in [package.json](file:///home/user/work/Relay/package.json#L21).
    - Captures a 16-second interactive run against the live console at `http://127.0.0.1:4200/` featuring concurrent multi-agent packet beats, WebGL fog drift, and facet tool interactions (unclipped lime selection and detail baseline alignment).
    - Encoded dual web assets in `docs/`:
      - [docs/relay-demo.mp4](file:///home/user/work/Relay/docs/relay-demo.mp4) (1.0MB, H.264 High Profile, faststart) for full 60fps truecolor playback with zero gradient banding on the dark `#070707` canvas.
      - [docs/relay-demo.webp](file:///home/user/work/Relay/docs/relay-demo.webp) (7.7MB, 24-bit RGB Animated WebP, loop=0) for universal Markdown image fallback.
      - [docs/demo-topology.png](file:///home/user/work/Relay/docs/demo-topology.png) (430KB) high-resolution crisp poster frame.
    - Updated [README.md](file:///home/user/work/Relay/README.md#L6-L10) with resilient hybrid `<video>` embed including `<img src="docs/relay-demo.webp">` fallback and static poster.
