# Native live-session slice

## API mapping

| Existing behavior | Native implementation |
| --- | --- |
| Full session snapshot | `LiveFleet::snapshot` → `session_snapshot(..., SnapshotMode::Full, ...)` |
| Conversation refresh | `LiveFleet::conversation_snapshot` → the same authoritative session and chat subscriptions, with changeset reads intentionally omitted |
| Files / changes refresh | `LiveFleet::snapshot` and `LiveFleet::preview` |
| Topology facet controls | Facet buttons keep hover/click behavior and now update their node facet from Enter or Space when keyboard-focused |
| UI snapshot request correlation | `SnapshotRequest.request`, host ID, URI, and host generation are checked before rendering |
| Changeset selection | Files tab renders advertised static, per-turn, and turn-pair changesets; turn parameters expand to validated URIs before refresh; turn-parameter rows use the source's `Turn`/`From`/`To` labels with `N · message` turn options (message trimmed to 64 chars with ID fallback for row layout) |
| Queue turn | Live conversation composer dispatches `chat/pendingMessageSet` after host, session, chat, generation, interactivity, and session-lifecycle validation; `failed`/`creating` lifecycles surface the source `chatPromptPolicy` reasons (`Session creation failed on the host` / `Session is still starting`) in place of the composer, a missing chat surfaces `Chat is not ready` (the `!chat` policy reason, which the web never renders as text but throws from `queueTurn`), and the worker re-validates lifecycle before dispatch (a native-only race guard — the web never reaches dispatch with a disabled composer). A present but non-writable chat reports the source interactivity reason verbatim (`This chat is read-only/hidden. The host does not accept user prompts here.`), mirroring `chatPromptPolicy` precedence behind the lifecycle reasons; the worker's interactivity rejection uses the same interpolation (the `queueTurn` `\|\|` fallback never fires — every deny branch carries a reason); the wire value is `read-only` with a hyphen per the AHP schema |
| Load older turns | Conversation exposes `Load earlier turns` when the host provides `turnsNextCursor`, validates the session/chat/cursor, dispatches `fetchTurns`, and refreshes from the authoritative snapshot after `chat/turnsLoaded` |
| Model selection | Native conversation composer lists the selected provider's advertised models, preserves the host default, and sends an optional `ModelSelection` on `chat/pendingMessageSet` |
| Chat input requests | Native conversation drawer renders text, numeric, boolean, single-select, and multi-select requests, including keyboard-selectable options, validated select free-form responses, and safe HTTP(S) review-link display, then dispatches `chat/inputCompleted` through the live worker. The request title falls back to `Agent requires input` and question legends fall back past empty titles, mirroring the source `||` chains exactly. Answer validation reports the source literals verbatim — `Complete <title‖message>` for missing required answers, `Invalid number for <id>` for unparseable/non-finite/non-integer numbers (parsed with a `Number(text)` mirror: surrounding whitespace ignored, `0x`/`0b`/`0o` prefixes accepted, whitespace-only yields 0 because the web's non-empty filter keeps it), `Invalid answer for <title‖message>` for every other constraint violation (text length counted in UTF-16 code units like the web `.length`, numeric ranges, option membership/count), and `Unsupported input question kind: <kind>` for a filled unknown-kind question — mirroring `readAnswer` parse errors before `validateInputAnswers` range errors. Unknown-kind questions render, pre-fill, and submit as text inputs (the web `session.tsx` text fallthrough), so an empty unknown skips or reports `Complete …` when required and a missing question list accepts with an empty answer map (`request.questions || []`). Untouched booleans stay unanswered (rendered `Choose an answer`, mirroring the web select placeholder) and submit skipped when optional or `Complete …` when required — never a fabricated `false` |
| Create session | Provider drawer resolves host session options, validates workspace/config/trust inputs, exposes keyboard-focusable provider/trust controls with Enter/Space activation, then dispatches `createSession` and opens the authoritative session URI. Its `New session` entry is gated on a live worker plus a connected host that still advertises the provider — mirrors the `!liveActive \|\| advertised === false \|\| busy` disable, with the worker re-validating advertisement before dispatch. Config fields mirror `ConfigField`/`readConfig` closely: read-only and unsupported types render as static `Managed by host` / `Advanced host setting` rows (with `· Required` and description) and keep their resolved values; string arrays edit as one entry per line, objects and non-string arrays as pretty-printed JSON with the source `Invalid JSON for host option …` / `Invalid type for host option …` / `Invalid host option …` errors; empty fields keep the previously resolved value; static enums render keyboard-focusable option rows (host-default placeholder plus one row per choice, labeled `enumLabels[i] || String(choice)`) mapping selection by index, plain booleans render Yes/No rows, both with Enter/Space activation like the chat input selects and strict `Invalid host option …` index validation; dynamic enums stay free text; field order is sorted by name because the deserialized map carries no host order; trust, refresh, empty-catalog, no-agents, and empty-schema strings are source-verbatim. |
| Dispose session | Session Info tab requires an explicit confirmation, dispatches `disposeSession`, and closes only after the host catalog confirms the session is gone |
| Session Info status | Info status prefers the authoritative snapshot when its lifecycle is `failed`, otherwise the cached summary; mirrors `sessionState(detail?.lifecycle === "failed" ? detail : summary)` |
| Host failure details | Live host drawer surfaces the terminal `Failed` state's error; transport failures map to the WebSocket handshake hint, other errors verbatim — mirrors the `lastError` mapping in `app.tsx` |
| Tool approval text | Approval title/body fall back through `confirmationTitle → displayName → "Tool confirmation"` and `intention → invocationMessage → toolName → "Review this tool request"`; advertised approve/deny options render as their own buttons, otherwise Accept/Reject result (result review) or Approve once/Deny fallbacks render — disabled when the host advertised only other option kinds; both buttons are keyboard-focusable. In-flight confirmation relabels the buttons `Submitting…`/`Waiting…` as a double-submit guard (the web dispatches synchronously with no pending UI); the title/intention render with no extra header (matches the web panel), and parameters render always expanded under a `Tool parameters` label (the web collapses them in `<details>`) |
| Tool authentication | `toolAuthentication` attention entries render the web `Attention` fallthrough verbatim (the kind plus `Resolve this request in the agent host.` with no in-app action), mirroring the `attentionFor` filter which admits only `toolConfirmation`, `chatInput`, and `toolAuthentication` (`toolClientExecution`/unknown kinds render nothing) |
| Topology empty states | Live and example maps render the shared `+` empty state: `Your fleet starts here` plus the connect hint when no hosts exist, otherwise `No matching nodes` plus the query hint — mirrors the `hosts.length` branch in `topology.tsx`; the over-cap line matches the source wording verbatim (`Showing X of Y sessions. Open Sessions for the full list.`). The Sessions catalog mirrors the `sessions.length` branch exactly (`No sessions` on an empty catalog, `No matching sessions` on an empty filter); the keyboard-controls dialog lists Arrow-keys node movement and Space preview alongside Enter/Esc, mirroring the screen-reader help text in `topology.tsx` (`Use arrow keys between nodes, Space to preview, Enter to open controls`); the Esc row reads `Close a dialog, clear selection or search` verbatim, matching the native Escape handling which clears search in both the focused and unfocused branches |
| Review changed file | Files tab exposes review-capable changesets, dispatches `changeset/filesReviewChanged` through the live worker after validating the changeset/file snapshot, and refreshes the authoritative snapshot |
| Reload recovery | Web "Reload Relay" (shown when `eventStreamHealthy` is false) maps to native "Restart worker" — in the host drawer when the worker died mid-session, and on the live stage whenever the worker is unavailable with no drawer open; the fresh worker drops in-memory hosts exactly like a page reload drops the in-memory fleet |
| Session ordering | Live catalog and `live::graph` iterate hosts and sessions in stored fleet order (`fleet.sessions()` / `aggregatedSessions`), which neither the source catalog nor `buildGraph` re-sorts; the topology's stable priority sort preserves that order within each group — no recency re-sort. The one `Fleet.sessions()` behavior not projected into native rows is the selected-session detail merge (fresh-session unshift plus lifecycle/status/activity overlay); native `SessionView` carries no lifecycle, so a `creating` selected session falls back to status bits in cap ordering — visible only past the 16-node cap |
| Topology graph | `example::graph_for_hosts` mirrors `buildGraph` constant-for-constant (128×32 cards, 320-wide lanes, 24 gap, 2 columns, hostY 24 / providerY 80 / gapY 16 / sessionGapY 8, 16-node cap, width/height floors 680/176, elbow bends −18/−10, stale edges when disconnected): stable priority sort on the bit-derived state with the host/provider/session grouping, unadvertised-provider backfill, query match fields, and skip-branch logic all equivalent; `topology_focused_node` is the `topologyFocusId` focused→selected→first fallback; arrows/Home/End/Enter/Space match (`Space` sets the selection in both — the web wires `selectedId` to the preview node); `query_terms` mirrors JS `\s` (BOM splits, NEL excluded); `status_of` bit logic is identical |
| Error status | Header shows the web `Issue` affordance while a live error is set; it opens a `STATUS` dialog rendering the current error (or `No current error.` when clean) with a `Dismiss` that clears it and returns focus — mirrors the web `status` drawer branch in `app.tsx`. The stage-level error banner carries the same `Dismiss` (the web banner's dismiss path); both clear `live_error` without touching fleet state. `Esc` closes the dialog first with focus restoration, background tab stops exclude it, Tab cycles between the dialog close and Dismiss controls, and `/`/`?` stay inert while it is open, matching the shortcuts-dialog plumbing |
| Error-string parity | User-facing error literals from connection, validation, queue, review, and model/lifecycle flows are source-verbatim against `src/fleet.mjs` (`Host name is required`, `An AHP host access token is required`, `Host is already connected. Select it, or remove it to change its endpoint.`, `A prompt is required`, `This chat does not accept user prompts`, model advertisement/policy, lifecycle reasons — none take a trailing period except the already-connected guidance sentence, which keeps its source period). Connect-endpoint validation mirrors `safeEndpoint` precedence (endpoint first, then name, then token): `Enter a WebSocket URL without embedded credentials or query parameters` and `Remote hosts require encrypted wss://` are source-verbatim, including the `ws:`-only-for-localhost/`127.0.0.1`/`[::1]` rule; `Enter a valid WebSocket URL.` is native-only — the web's `new URL` TypeError surfaces via `safeError`, so no source literal exists. Guards the types make unrepresentable are absent rather than paraphrased: `Not a tool confirmation` / `Not an input request` (typed request structs built at a single UI site each; `snapshot.approval` is only `Some` for `ToolConfirmation` entries), `Invalid input response` (closed `ChatInputResponseKind` enum; values originate only from the three response buttons), and `Unknown input question:` / `Input answer is not submitted:` / `Invalid input answer:` (the answers map is built by iterating the request's own questions with typed `Submitted`/`Skipped` values, so unknown IDs and malformed shapes cannot occur), `Select a session` (every worker `Command` carries explicit host/session strings and action buttons only render with a selection, so a null-selection dispatch has no path), `Channel is not owned by the selected session` (chat URIs are built only from the selected session's own snapshots, and the worker re-validates chat ∈ session chats with the source-verbatim `Chat is not part of the selected session` plus generation guards), and `AHP event stream unavailable…` ×2 (no event-stream concept natively; worker death surfaces `Host connection worker stopped.` with the `Restart worker` recovery stage). Native-only decode failures `Host returned invalid base64 content` / `Host returned invalid UTF-8 content` have no source literal — the web's `atob`/`TextDecoder` throw platform exceptions surfaced via `safeError` — while the SDK's typed data/encoding make `Host returned no file content` / `Unsupported file encoding from host` unrepresentable. Config inputs show `One entry per line` only on string-item arrays, matching the web placeholder exactly |

## Deliberate differences

- Conversation opens and refreshes do not load changeset resources. The returned
  snapshot marks files as `Files were not loaded; refresh the Files tab.` instead
  of pretending file state is current. Entering the Files tab triggers the full
  changeset refresh.
- Cancellation validates the selected host generation, session, chat, and active
  turn immediately before dispatching `chat/turnCancelled`. The UI keeps the
  action pending until a fresh authoritative snapshot confirms that the active
  turn has cleared; transport acknowledgement alone is not treated as
  completion.
- Queueing validates the selected session owns the chat and that the chat is not
  read-only or hidden. The composer waits for the worker event before clearing
  its input and refreshes from an authoritative snapshot afterward.
- The topology map renders without the WebGL lightfield backdrop
  (`ui/topology-fx.mjs`: conduit glow, node halos, selection perimeter, event
  pulses, grid/scanline treatment). GPUI exposes no shader surface for it; the
  authoritative layer it illuminates — nodes, solid/dashed edges, grid,
  empty/over-cap states, focus/keyboard/pan — is ported, and the lightfield
  itself carries no interaction or state.
- Non-visual ARIA semantics (`aria-label` on the tab strip, icon buttons, preview
  and topology regions, plus `role="alert|status|log|region|group|button"`)
  are not reproduced: the pinned `gpui 0.2.2` crate exposes no accessibility
  tree API (no `aria`/`accessibility` surface anywhere in its source), so
  there is nothing to attach them to. Screen-reader help that the web renders
  as visible-adjacent text (topology movement help, the `Esc` row) is already
  ported as visible text in the keyboard-controls dialog.
- Turn token usage (` · N tokens`) is not shown: the web reads the untyped
  `turn.usage?.totalTokens` field, which the official SDK 1.0.0 `UsageInfo`
  type does not carry (only input/output/cache-read/model/meta), so it never
  survives typed deserialization. Summing input+output would display numbers
  the web never shows; the host remains authoritative for usage.
- File-preview reads are bounded by a 15-second timeout with stable native-only
  errors (`File content request timed out.` / `Could not read file content
  from the host.`): the web `readEdit` awaits the `resourceRead` request
  unboundedly and surfaces the raw SDK/transport rejection, so no source
  literal exists for either failure — the same native-only status as the
  decode failures recorded in the error-string parity row.
- Session-drawer error displays apply `safe_error` (token redaction) with the
  source fallbacks verbatim: snapshot failures render `<message>. The host may
  have restored this session under a new ID.` with the `fleet.mjs`
  `Session data unavailable` fallback, preview failures render the bare
  `safeError(error)` (`Unknown error` fallback), and the failed-lifecycle
  creation error renders bare with `The host could not initialize this
  session.` — no `Session snapshot unavailable:` / `File preview unavailable:`
  / `Session initialization failed (snapshot):` prefixes, and the two
  `freshness unverified` subtitles the drawer carried — none of which the web
  renders in the session drawer. The worker's own
  snapshot/guard literals (`Could not load the session snapshot.`,
  `Could not release the session subscription.`, `Host returned no session
  snapshot.`, `Host returned a snapshot for a different resource.`,
  `Host returned an unexpected snapshot type.`, `Host connection changed;
  reopen the session.`, `Session is no longer advertised by the selected
  host.`, `Changeset is unavailable.`, `Snapshot request superseded.`)
  remain native-only: the web surfaces the raw subscribe/dispatch rejection
  via `slot.error = error?.message || "Session data unavailable"`, while the
  worker channel carries `&'static str`. Carrying raw SDK messages across
  would require re-plumbing worker errors to owned strings; recorded as open
  work, not done here.
- Timestamps render as fixed local `YYYY-MM-DD HH:MM:SS TZ` instead of the web
  `toLocaleString()` output, and only RFC 3339 / `YYYY-MM-DD` /
  `YYYY-MM-DDTHH:MM:SS[.fff]` inputs parse (anything else the browser's `Date`
  accepts still shows `Not reported`): locale-dependent formatting has no
  deterministic native equivalent, and the fixed format keeps every timestamp
  comparable across machines.
- Chat input answers are held locally until the user submits, declines, or
  cancels. A submitted response waits for the worker acknowledgement and then
  refreshes the authoritative session snapshot; stale acknowledgements cannot
  clear a different request.
- Older-turn pagination validates the current session, chat, host generation,
  and opaque cursor before requesting `fetchTurns`; the UI waits for the command
  result and then re-reads the chat snapshot rather than fabricating a second
  transcript.
- Input validation covers supported required, text-length, numeric-range, integer,
  selection-count, and select free-form constraints before dispatch. The host
  remains authoritative for any protocol capability the UI does not expose.
- Model selection is validated against the current host generation and provider
  advertisement before dispatch; disabled models are rejected while the host
  default remains the no-selection state.
- Session creation validates the host generation, provider advertisement,
  workspace URI, required resolved options, and trust prerequisites before
  dispatch; resolved options are tied to the exact workspace URI, and the
  resulting session is opened only from the host response.
- Session disposal validates the host generation and session advertisement
  before dispatch; transport acknowledgement alone is not treated as removal,
  and stale or failed disposal leaves the drawer available for recovery.
- File review actions are only rendered when the selected changeset advertises
  review capability. The worker revalidates the selected changeset and file
  before dispatch, and the UI waits for the worker result before refreshing.
- Numeric parsing mirrors JavaScript `Number(text)` in both chat-input answers
  and session-config numeric options (surrounding whitespace, `0x`/`0b`/`0o`
  prefixes, whitespace-only yields 0). Exotic divergences: Rust trims U+0085
  (NEL) while JS trim does not, and >2^53 hex/binary/octal digits can differ
  in the last ulp from the web's single-rounding parse.
- Text truncation (`clip`) counts Unicode scalar values and never splits a
  surrogate pair; the web `clip` counts UTF-16 code units. Byte output differs
  only when truncation lands on an astral-plane character (emoji); a lone
  surrogate is unrepresentable in a Rust `String`, so exact reproduction is
  impossible.
- Empty-workspace fallbacks mirror the two source `tail` variants exactly: the
  topology cards fall back to `Not reported` (the local `tail` in
  `topology.tsx`), while the preview panel, file rows, session rows, and Info
  tab fall back to `—` (the shared `tail` in `helpers.ts`).
- Example (preview) mode renders muted one-liners where the web renders the
  same controls disabled: the message composer (`Message composer unavailable
  in example mode.` for the disabled textarea/model-select/Cancel/Queue form),
  the provider drawer (`New session unavailable in example mode`), the review
  checkbox (`Reviewed · unavailable in example mode` for the disabled
  checkbox), and session disposal (`Dispose session · unavailable in example
  mode` for the disabled button). The web's disabled controls submit nothing
  in example mode, so the native lines carry the same read-only state without
  dead input plumbing; every other example string (status, tokens, file sample,
  `Host advertises no file changes.`) is data-verbatim, and example timestamps
  carry the same instants under the locale-format difference recorded above.
- Shell chrome (`src/routes/`, `src/router.tsx`, `src/serve.mjs`): the single
  `/` route renders `RelayApp` with no guards, so the native root view boots
  `Relay` directly — no router to port. `AuthProvider` is an explicit
  passthrough (its own file comment says so) and `RelayApp` reads no auth, so
  there is no gating divergence. `PreviewHostBridge`, the PWA manifest/icons,
  and the description/theme-color meta are preview-platform chrome, not app
  content. `serve.mjs` is dev/demo static-asset tooling with no UI surface.
  The route error boundary (`Something went wrong` plus the error message) has
  no native equivalent — a window-open failure goes to stderr and quits. The
  document title (`Relay · Fleet`) is carried as the native window title.

## Verification

- `cargo +stable check --locked --manifest-path native/Cargo.toml`: passed.
- `cargo +stable clippy --locked --manifest-path native/Cargo.toml -- -D warnings`: passed with no clippy lints; Cargo reported the existing `proc-macro-error2` future-incompatibility warning.
- `cargo +stable build --locked --manifest-path native/Cargo.toml`: passed.
- Offline resolution was not available in this run because the local cache
  lacked the `ahp` package index entry; locked online resolution was used.
- `cargo +stable fmt --manifest-path native/Cargo.toml -- --check`: passed.
- Session catalog create action selects the first connected host and opens the
  provider/config flow; the action is unavailable when no connected host exists.
- Tracked-diff whitespace check: `git diff --check` passed. The untracked
  native files were covered by `cargo +stable fmt --check` rather than that Git check.
- Automated tests were not run; the task did not authorize them.
- Isolated AHP 1.0.0 host probe: passed. It verified session/provider/chat
  matching, explicit rejection of wrong chat and mismatched generation, a stale
  turn rejection, active-turn cancellation dispatch, queued follow-up dispatch,
  and an authoritative post-cancellation snapshot with no active turn. The host
  was disposable and stopped after the probe.
- Rebuilt binary re-launched in a fresh isolated nested session: single client
  mapped at `1200x800`, visible, empty log; client list empty after shutdown
  and the session stopped. Pixel capture was not possible (`grim` hung against
  the nested socket, the known screencopy-unanswered case); no outer-desktop
  capture attempted.
- Native GPUI UI interaction was not directly verified: the isolated nested
  session exposed no target window to the installed LCU runtime. Static
  compilation and the worker-level isolated probe do not prove rendered focus,
  tab activation, or visual timing.
