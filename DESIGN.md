# Relay Design System

## Authority and scope

This is Relay's portable visual contract. The rebuilt App Builder surface uses React, locally bundled Nexvyn UI components, native SVG and CSS. `src/styles.css` owns its executable theme variables. The original standalone Lit surface remains in `ui/`; its historical density specifications below do not override the Nexvyn rebuild. Neither surface requires private tooling, remote fonts or a CDN. The official AHP client SDK supplies host/session state; the browser must not maintain a duplicate durable data model.

### Nexvyn rebuild

The active React entry is `src/routes/index.tsx`, with fleet presentation in `src/features/relay/`. Preserve Nexvyn's component-default geometry: medium buttons have a 44px minimum height, 16px inline padding and 8px content gap; icon buttons use the supplied 44px icon variant; subtle tabs retain their 6px list padding, 12px/6px item padding and 8px panel gap. Requested chrome is compact: a 14rem host rail with static selected rows and directional chevrons, 13rem by 28px search, 24px Keys and preview controls, an in-flow selection strip, and a 22rem drawer; graph cards are uniform 128px by 32px. The strip sits above the topology, never overlaps nodes, and expands or releases layout space as selection changes. Narrow or coarse-pointer controls retain 44px targets. Theme tokens translate corners to square geometry and surfaces to black/neutral. Use borderless ghost actions, lime primary-action text, visible focus and actual disabled semantics. Do not reapply the original 18–36px control-density overrides to general components.

The sidebar chevron stays anchored to the collapsed rail's fixed slot: opening or closing the sidebar must not move it horizontally or vertically. Align the Hosts heading to the same centerline with a consistent trailing inset.

Retain the brand bar, collapsible host rail, topology divider, floating search, in-flow node preview, bottom session catalog and modal details drawer. Their containers may reflow, particularly on mobile. Connected sessions, queued turns, approvals and changes continue through `src/fleet.mjs` without a second durable model.

Use a dense, flat operational workspace: black and neutral surfaces, readable light text, deliberate one-pixel divisions, lime activity and square corners. Content and **host-authoritative AHP state** determine the display. Show actual host/provider/session topology and explicit connecting, live, stale, empty, permission-needed and failure states. Never invent agent traffic, readiness, inference, model results or completion. Session creation is not proof of a completed turn.

The shell is a map, not a diagnostic report. Topology nodes use a two-compartment frame with a vertical three-icon mini-sidebar (`X`, `Y`, `Z`) on the left and the primary node body (`Name Here` and a truthful bottom state/signal bar `-ProgressBar-`) on the right:

```text
+---+---------------+
| X |  Name Here    |
| Y |               |
| Z | -ProgressBar- |
+---+---------------+
```

Hovering or pinning an icon (`X`, `Y`, `Z`) swaps the main compartment to that authoritative facet (such as state, activity, workspace, provider, or session count) without expanding the node's footprint; the bottom `-ProgressBar-` reflects actual host/session state, never synthetic completion percentages. Do not render native browser `<title>` tooltips on nodes or pointer-hand cursors on interactive elements. Clicking a node opens a compact preview with an SVG `×` close button and a `Control →` action; in the rebuild it occupies an in-flow strip above the topology rather than covering nodes. `Enter` is the sole keyboard shortcut to open controls from a selected node. Put full transcripts, forms, and long identifiers behind deliberate selection in the right-hand drawer. The session catalog is an optional compact list, collapsed by default. No persistent metrics tiles, verbose inline topology prose descriptions, redundant legends or empty inspector panels.

## Token structure

- `c-*`: palette and color roles.
- `f-*`: font families; `fs-*`: type sizes; `fw-*`: weights; `lh-*`: line heights.
- `sp-*`: spacing only.
- Dimensions use `w` / `h` for width / height, `bd` for border, `ctrl` for control and `bp` for breakpoint. Keep each role independent.
- Use concise role names; omit redundant project prefixes. Do not merge independent roles to shorten names.

Alias only deliberate relationships. Equal values are not evidence that roles should change together. Icons do not alias type sizes or spacing. Title bounds do not alias pane padding. Target width does not alias target height. Border width, pane seam, focus width, focus offset, slider track and press distance remain independent, even when values coincide.

Components may alias authoritative roles rather than retain a competing palette. Optional interoperability names such as `background`, `primary`, `input`, `ring` and `sidebar` may alias these roles when consumers need them. They are not framework dependencies or another configuration engine. No generated token file is required by the application.

## Wonder variables

The HyprRice design file (Wonder organization `salty-pugzy`, page `Hyprland`, branch `main`) authors this system's sizes, fonts and colors. It holds 72 variables; every name below is the file's own, shown as its `cssVar`, and reads return resolved CSS lengths and colors. The role names in the sections below remain the executable contract for this repository's CSS.

### Colors

| Variable | Value |
| --- | --- |
| `var(--accent-1)` | `#c4ff00` |
| `var(--accent-2)` | `#c9a227` |
| `var(--accent-3)` | `#00c4a0` |
| `var(--accent-var)` | `#00c4a0` |
| `var(--active)` | `#212121` |
| `var(--bg-1)` | `#000000` |
| `var(--bg-2)` | `#070707` |
| `var(--border)` | `#070707` |
| `var(--border-focus)` | `#c4ff00` |
| `var(--critical)` | `#f43434` |
| `var(--focus-ring)` | `#c4ff00c2` |
| `var(--glass)` | `#000000f7` |
| `var(--glow)` | `#c4ff00bf` |
| `var(--hover)` | `#1c1c1c` |
| `var(--muted-1)` | `#c0c0c0` |
| `var(--muted-2)` | `#c2c2c2` |
| `var(--separator)` | `#303030` |
| `var(--shadow)` | `#000000` |
| `var(--surface-1)` | `#0f0f0f` |
| `var(--surface-2)` | `#0f0f0f` |
| `var(--text)` | `#f4f4f4` |
| `var(--text-bright)` | `#ffffff` |
| `var(--warning)` | `#ffc400` |

`accent-var` aliases `accent-3`. Every other color value above already appears in the palette below under a role name, except `muted-1` (`#C0C0C0`, where the palette's `c-muted` is `#9A9A9A`) and the three translucent colors `glass` (`#000000f7`), `glow` (`#c4ff00bf`) and `focus-ring` (`#c4ff00c2`), which have no role equivalent.

### Typography

| Variable | Kind | Value |
| --- | --- | --- |
| `var(--font-l)` | size | `17px` |
| `var(--font-m)` | size | `14px` |
| `var(--font-mono)` | size | `13px` |
| `var(--font-s)` | size | `13.5px` |
| `var(--font-waybar)` | size | `15px` |
| `var(--font-xs)` | size | `13px` |
| `var(--l-family)` | family | `Rajdhani` |
| `var(--l-weight)` | weight | `500` |
| `var(--m-family)` | family | `Poppins` |
| `var(--m-weight)` | weight | `500` |
| `var(--mono-family)` | family | `DM Mono` |
| `var(--mono-weight)` | weight | `500` |
| `var(--s-family)` | family | `DM Sans` |
| `var(--s-weight)` | weight | `500` |

The six type sizes carry a grouped name in the file (`font/xs`), whose `cssVar` replaces the slash with a hyphen. These families differ from the role contract below, which uses `f-sans` and `f-mono` with local fallbacks; a declared family is not proof that the face is installed.

### Geometry and sizes

| Variable | Value |
| --- | --- |
| `var(--dot-size)` | `6px` |
| `var(--h-comp)` | `30px` |
| `var(--h-switch)` | `26px` |
| `var(--h-track)` | `6px` |
| `var(--h-waybar)` | `40px` |
| `var(--launch-i)` | `31px` |
| `var(--l-icon)` | `30px` |
| `var(--l-pad)` | `20px` |
| `var(--m-icon)` | `20px` |
| `var(--m-pad)` | `10px` |
| `var(--notif-i)` | `27px` |
| `var(--p-switch)` | `3px` |
| `var(--r-sw-thumb)` | `8px` |
| `var(--r-switch)` | `10px` |
| `var(--radius)` | `4px` |
| `var(--s-compact)` | `60px` |
| `var(--s-icon)` | `16px` |
| `var(--s-pad)` | `4px` |
| `var(--s-sl-thumb)` | `16px` |
| `var(--s-step-icon)` | `7px` |
| `var(--s-sw-thumb)` | `18px` |
| `var(--tray-gap)` | `6px` |
| `var(--w-btn)` | `130px` |
| `var(--w-chip)` | `50px` |
| `var(--w-edge)` | `1px` |
| `var(--w-input)` | `60px` |
| `var(--w-label)` | `60px` |
| `var(--w-panel)` | `420px` |
| `var(--w-row)` | `400px` |
| `var(--w-select)` | `120px` |
| `var(--w-slider)` | `100px` |
| `var(--w-stepper)` | `20px` |
| `var(--w-switch)` | `44px` |
| `var(--xs-icon)` | `14px` |
| `var(--xs-pad)` | `2px` |

`w-input` and `w-label` alias `s-compact`.

## Palette and state pairs

| Token          | Value     | Role                                                   |
| -------------- | --------- | ------------------------------------------------------ |
| `c-bg`         | `#000000` | Canvas                                                 |
| `c-bg-alt`     | `#070707` | Recessed regions and default controls                  |
| `c-surface`    | `#0F0F0F` | Raised neutral surfaces                                |
| `c-hover`      | `#1C1C1C` | Neutral hover                                          |
| `c-active`     | `#212121` | Selected neutral rows                                  |
| `c-bd`         | `#070707` | Quiet decorative division                              |
| `c-bd-soft`    | `#141414` | Inner decorative division                              |
| `c-sep`        | `#303030` | Section and record separators                          |
| `c-fg`         | `#F4F4F4` | Ordinary text and record payloads                      |
| `c-bright`     | `#FFFFFF` | Highest-priority emphasis                              |
| `c-secondary`  | `#C2C2C2` | Supporting text                                        |
| `c-muted`      | `#9A9A9A` | Metadata and current graph lines                       |
| `c-muted-base` | `#7F7F7F` | Necessary control boundaries and uncertain graph lines |
| `c-accent`     | `#C4FF00` | Activity, focus and primary action                     |
| `c-on-accent`  | `#000000` | Foreground on lime; independent of canvas color        |
| `c-data-gold`  | `#C9A227` | Optional labeled comparison data                       |
| `c-data-teal`  | `#00C4A0` | Optional labeled comparison data                       |
| `c-danger`     | `#F43434` | Error marker                                           |
| `c-warning`    | `#FFC400` | Warning marker                                         |

`c-input` deliberately aliases `c-muted-base`; `c-focus` aliases `c-accent` (focused inputs, selects and textareas replace their 1px border color with `c-focus` rather than adding a second outer frame).

| State role                                           | Pairing                                 |
| ---------------------------------------------------- | --------------------------------------- |
| `c-ctrl-bg` / `c-ctrl-fg` / `c-ctrl-bd`              | `transparent` / `c-fg` / `none`         |
| `c-ctrl-hover-bg` / `c-ctrl-hover-fg`                | `transparent` / `c-bright`              |
| `c-primary-bg` / `c-primary-fg` / `c-primary-bd`     | `transparent` / `c-accent` / `none`     |
| `c-primary-hover-bg` / `c-primary-hover-fg`          | `transparent` / `c-bright`              |
| `c-link-current` / `c-link-uncertain` / `c-link-event` | `c-muted` / `c-muted-base` / `c-accent` |

Action buttons are borderless (`border: 0; background: transparent;`) and sit directly against the background accompanied by a supporting inline `<svg>` icon; primary actions use lime text (`c-accent`) shifting to `c-bright` on hover, and danger actions use `c-danger` text. Selection on list rows uses `c-active`, a visible lime inset marker and a contrast-checked edge. Error text uses `c-fg` plus a separate red marker, not tiny red text on arbitrary surfaces. Current, offline and confirmed-turn links retain distinct stroke/dash treatments. Color never supplies the only state explanation; status legend markers (`ACTIVE`, `ATTENTION`, `IDLE`) and node mini-sidebar icons use real inline `<svg>` vector geometry rather than font-dependent Unicode glyphs.

Normal text needs at least 4.5:1 contrast and essential non-text boundaries/indicators at least 3:1 on their actual default, hover and selected surfaces. Decorative seams need not meet the essential-boundary threshold. `c-muted-base` is a boundary color, not ordinary text on hover charcoal. Disabled controls use real disabled semantics and opacity .55; they have no hover or press effect.

Standards: [WCAG 2.2 Contrast (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) and [Non-text Contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html).

## Typography

Two family definitions only:

- `f-sans`: `"IBM Plex Sans", "Plus Jakarta Sans", "Segoe UI", system-ui, sans-serif`.
- `f-mono`: `"Geist Mono", "IosevkaTerm NFM", "CommitMono Nerd Font", "IBM Plex Mono", "SF Mono", "Consolas", ui-monospace, monospace`.
- `f-legend` aliases `f-mono` for operational legends.

Local font availability may select a fallback. A declared family is not proof that the preferred face is installed. No private font or private-use glyph is required to understand an icon.

| Role                                                                                    | CSS size                                               | Pixel equivalent at a 16px root |
| --------------------------------------------------------------------------------------- | ------------------------------------------------------ | ------------------------------- |
| `fs-xs`: metadata, IDs, states, legend and keyboard hints                               | `.75rem`                                               | 12px                            |
| `fs-sm`: body, payloads, node names and all interactive labels, including quiet buttons | `.875rem`                                              | 14px                            |
| `fs-md`: section/pane headings                                                          | `.875rem`                                                | 14px                            |
| `fs-lg`: wordmark and stronger headings                                                 | `1rem`                                                   | 16px                            |
| `fs-title`                                                                              | `clamp(var(--fs-title-min), 1.6vw, var(--fs-title-max))` | Independent 16px/20px bounds    |
| `fs-metric` / `fs-metric-sm`                                                            | `2rem` / `1.5rem`                                      | 32px / 24px                     |

`fw-ctrl` is 500, `fw-support` 400, `fw-head` 600 and `fw-brand` 650. `lh-body` is 1.5, `lh-label` 1.35, `lh-head` 1.2 and `lh-read` 1.7. Specific operational rows may use their existing readable line heights; do not generate a second type scale from fractional capture values. Preserve tabular numerals for changing counts and measurements. Type and content spacing use relative units so user text scaling can reflow the interface.

## Geometry and layout

All interface corners are `r-none: 0px`. No nested cards, panel shadows, gradients, blur, glass or ornamental glow on shell surfaces. **The topology is the exception:** its existing WebGL SDF lighting, reactive grid, illuminated conduits, scanlines and grain are intentional and must remain confined to actual graph geometry, selection and host-confirmed events. The 20px topology grid extends across the full `.map-viewport` as an infinite pannable canvas without abrupt stage edges. No idle light animation or synthetic network traffic. Native SVG remains the accessible interaction and non-WebGL fallback layer.

| Independent dimensions                           | Values at a 16px root                                                                        |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| `ctrl-min-h` / `ctrl-min-w`                      | 36px / 36px                                                                                  |
| `touch-min-h` / `touch-min-w`                    | 44px / 44px                                                                                  |
| `btn-w` / `btn-h`                                | Shared 60px / 36px borderless action-button body; height becomes 44px for narrow/coarse layouts |
| `collapse-w` / `collapse-h`                      | 32px collapsed host rail stripe; 36px/44px for drawer/dialog close actions                   |
| `field-w` / `field-h` / `search-w` / `search-h`  | 208px password field / 36px form field height / 200px (`12.5rem`) search width / 20px search height |
| `session-select-w` / `session-select-h`          | 256px / 36px minimum height, bounded by available width                                      |
| `key-w` / `key-h`                                | Borderless right-aligned `<kbd>` hints in operator reference                                 |
| `icon-xs` / `icon-sm` / `icon-md` / `icon-brand` | 12px / 16px / 20px / 24px inline `<svg>` icons                                               |
| `node-icon` / `node-icon-sm`                     | 16px left node tool rail with 3 SVG facet buttons                                            |
| `bw` / `sp-pane`                                 | Independent 1px border / 1px seam (all borders everywhere 1px)                               |
| `focus-w` / `focus-gap`                          | Independent 1px / 2px                                                                        |
| Slider track / press-distance authoring slots    | Independent 6px / 1px; no animation requirement                                              |

Compact desktop controls use the 36px targets, while the fleet search input uses a compact 20px height beside the `Keys ?` trigger. At 760px and below, or with a coarse pointer, controls use the independent 44px targets. Targets are real boxes, not overlapping invisible hit areas. Content can grow beyond minimum heights, and focus must remain visible inside scroll regions.

All ordinary action buttons are borderless with supporting inline SVG icons across default, primary, quiet, danger, hover, focus and disabled states; close buttons render only the `×` icon with an accessible label. Node buttons are data-bearing SVG groups: they retain their two-compartment frame (`16px` left tool rail + main body) with a `1px` dark border (`c-sep`) by default that shifts to `1px` light grey (`c-secondary`) when selected. Sessions, identities, metadata and section containers retain responsive width and wrapping. Runtime dimensions use rem so browser text preferences scale targets with their labels.

Raw spacing steps `sp-2/4/6/8/10/12/16/20/24/32/40/48` have those pixel equivalents, expressed in rem. `sp-xs/sm/icon/md/lg/xl` may alias steps 2/4/6/10/20/40 within spacing only.

- Controls: 6px block / 10px inline padding.
- Nodes: 16px left icon rail + compact body; 50%-scale floating node preview card (`max-width: 15.5rem`).
- Records: 16px insets; 6px row / 12px column gaps.
- Panes: 20px default insets, 12px narrow; group gaps use 24/40/48px as the actual composition needs.
- Metrics fill their grid tracks. Long identities and payloads wrap; records preserve payload line breaks and JSON indentation.

The page has a 1600px maximum content width with a collapsible host rail and full-width topology below a `1.75rem` (`28px`) top brand bar. When collapsed, the host rail is a `32px` vertical stripe along the left edge displaying an SVG `>` unfold chevron, vertically stacked `H o s t s`, and the host count; when expanded, it slides out as a resizable `14rem` default-width sidebar displaying host identity and state. `.sidebar`, `.map-head`, and `.side-drawer` share a unified `1.625rem` (`26px`) header strip whose `1px` bottom borders form a continuous horizontal hairline across the workspace. On the left end of `.map-head` (logically aligned with the Hosts rail), `+ Add host` expands an inline host connection tray (`#connect-panel.connect-form`) directly inside the separator with three `18px`-tall, `1px`-bordered inputs (`label`, `wss://host.example/ahp`, `token`), `Connect →`, and an SVG `×` close button—sliding out with a bounded `170ms` clip/translate reveal (disabled under `prefers-reduced-motion: reduce`) without shifting the separator height or opening the right drawer. The right side of `.map-head` houses any active turn indicator and the legend (`ACTIVE · ATTENTION · IDLE` with the compact `Keys ?` control on the right). Beneath `.map-head`, the canvas contains a floating `20px` search tile at `8px` offset (`top: 34px`), an optional floating node preview card (`max-width: 15.5rem`, `top: 62px`), and a slim `1.75rem` (`28px`) bottom session bar without a redundant viewport bottom separator. Clicking a node previews it in the floating card with inline `LABEL: VALUE` details and a right-aligned `Control →` action; clicking `Control →` or pressing `Enter` opens its status, diagnostics and connection management (or session **Conversation / Files / Info** views and the create-session form) in a matching resizable `14rem` default-width **modal right drawer** with an undimmed backdrop and symmetric desktop workspace inset. The drawer and inline host tray close on Escape or their `×` close controls without destroying a session selection. Keep the host token hint accessible but visually hidden. Use `minmax(0, 1fr)` and `min-width: 0`; keep graph panning within the map, not the page.

## State, motion and verification

Preserve a bounded SVG/CSS/WebGL pulse only for host-confirmed turn completion or cancellation. It does not depict ongoing readiness or current execution authority. The shader is drawn on topology/selection/scroll changes, with animation frames only during the bounded confirmation pulse. The WebGL canvas is viewport-sized and only illuminates visible host-confirmed graph geometry; inactive links never receive live neon treatment, while the full native SVG remains navigable. The Rust console renders the same light field from the same rules: `native/src/lightfield.rs` mirrors `ui/topology-fx.mjs` expression for expression, repaints only when the geometry it depends on changes, and animates only the bounded confirmation pulse. Do not add idle animation, springs, entrances or an animation runtime. Reduced motion keeps static state information and removes the moving dash treatment and shader.

Keep explicit loading, empty, disconnected, cached and error labels. Subscribed session/chat state may remain visible through transport failure only when clearly marked stale and controls disabled; reconnect must re-seed from authoritative snapshots. Never portray a queued prompt as accepted until the host echoes it, or a cancellation request as an ended turn until the host confirms it.

Inbound AHP event-stream failure is fatal to the current tab's authority: mark the view stale, disable all dispatch, and request a reload. Rejections do not advance the server sequence. Switching chats before a pending submission is acknowledged must preserve an explicit uncertainty warning, not imply completion. Hidden worker chats do not belong in the drawer; host-disposed sessions must disappear from the topology.

Verify the built Relay application, not only the design guide:

- Normal, hover, focus, disabled, selected, loading, empty, cached and error states.
- Tab/Shift+Tab, Enter/Space, Escape and existing topology navigation; no clipped focus.
- 320/360px, 760px and 1440px layouts, including opened drawers, their forms, tabs and coarse-pointer targets.
- Actual nonblack WebGL lightfield output when supported, visible routed conduits, SVG fallback and absence of idle animation.
- 200% text scaling, readable wrapping, and no unintended page-level overflow or clipped controls.
- Actual foreground/background contrast, stable selected data, committed-event pulse expiry and reduced motion.
- Authenticated AHP sessions/permissions, host disconnect/reconnect, strict CSP, local assets and host-owned file changes are verified through a real AHP server.

Static screenshots do not prove real disabled semantics, keyboard behavior, live data or responsiveness. Test the browser against an isolated AHP daemon and ACP fixture, then separately qualify real agent execution. Fixture output is not proof that an authenticated production model can infer, restore a session or make approved file edits.
