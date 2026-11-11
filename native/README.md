# Relay native

Rust GPUI fleet console. It reads `$AI_HOME/config/theme/tokens.json`, falling
back to `~/.ai/config/theme/tokens.json`. Canonical theme files are never written.

## Run

```sh
cargo +stable build --locked --manifest-path native/Cargo.toml
native/target/debug/relay-native
native/target/debug/relay-native --theme /path/to/tokens.json
```

The process opens the fleet console. `--theme FILE` selects a base theme and
does not modify that file.

## Topology light field

The console draws the browser console's topology light field. `src/lightfield.rs`
mirrors the WebGL fragment shader in `../ui/topology-fx.mjs` expression for
expression: signed-distance neon falloff for conduits and node housings, the
reactive grid, the selected perimeter and the bounded committed-turn pulse.
GPUI exposes no custom shader surface, so the field is rendered to BGRA pixels
and handed to GPUI as an image. It repaints when the geometry it depends on
changes, never on an idle animation loop; only the 1.4s host-confirmed pulse
animates.

```sh
cargo run --example lightfield -- /tmp/relay-lightfield
```

Writes `/tmp/relay-lightfield.png` and `/tmp/relay-lightfield.json`. The JSON
holds the exact shader uniforms for that frame, so the real WebGL shader can
render the same frame and the two images can be diffed.

## Live host probe

`src/live.rs` is the console's AHP client. The probe drives it headlessly against
a real host, which is how the topology's live path can be checked without the
window:

```sh
cargo run --example live_probe -- ws://127.0.0.1:<port> <token-file>
```

It starts the same `LiveFleet` worker the console starts, adds the host, waits
for the host to settle, and prints the hosts, sessions and graph the console
would draw.

Against a host whose agents carry the schema's required `description`, the probe
reports `connected` and prints the hosts, agents and graph the console draws.

An agent entry without that field makes every attempt end within milliseconds
with:

```text
failed to deserialize response: data did not match any variant of untagged enum SnapshotState
```

`SnapshotState` is an untagged enum and `AgentInfo::description` has no serde
default, so one missing field rejects the whole snapshot. The ACP bridge emits a
description only when the preset writes one
(`@ahpd/agent-acp` `agent.ts`), which is why the fixture preset in
`test/fixtures/isolated-hosts.mjs` sets it. The browser client tolerates the
missing field, so a fixture defect looks like a server-side connection failure
from the console alone.

```sh
cargo +stable fmt --manifest-path native/Cargo.toml -- --check
cargo +stable test --locked --manifest-path native/Cargo.toml
cargo +stable clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
```
