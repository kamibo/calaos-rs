# Repository Guidelines

## Project Structure & Module Organization
- Rust workspace with two crates:
  - `core/` (library): protocol parsing, IO controllers, rules engine, MQTT client, websocket server.
  - `server/` (binary): wires configs, channels, MQTT, IO controllers, and servers.
- Configs are external XML (IO) and XML/JSON (rules) files passed to the binary at runtime.
- Notable modules: `core/src/{mqtt_client.rs, io_context.rs, main_server.rs, config/*}`.

## Build, Test, and Development Commands
- Build: `cargo build` (workspace build).
- Tests: `cargo test` (runs unit and async tests).
- Run server:
  - `cargo run -p server -- <io_config.xml> <rules_config.xml> [--ssl_config_dir <dir>] [--no_input] [--no_output]`.
  - MQTT is configured via CLI flags:
    - `--mqtt-host` (default `localhost`)
    - `--mqtt-port` (default `1883`)
    - `--mqtt-username` (optional)
    - `--mqtt-password` (optional)
    - `--mqtt-discovery-prefix` (default `homeassistant`)
    - `--mqtt-node-id` (default `calaos`)
    - `--mqtt-keep-alive-sec` (default `30`)
  - Optional shutdown grace: `SHUTDOWN_GRACE_MS` (default 1000).

## Coding Style & Naming Conventions
- Follow standard Rust style (rustfmt). Prefer 4‑space indentation, no tabs.
- Naming: modules/files `snake_case`; types/traits `PascalCase`; functions/vars `snake_case`.
- Keep changes minimal and focused; avoid unrelated refactors.
- Avoid adding license headers; match existing logging via `tracing`.

## Testing Guidelines
- Use `#[test]` and `#[tokio::test]` for async paths. Keep tests close to code (same file/module) unless integration requires otherwise.
- Name tests descriptively: `feature_behavior_expected_outcome`.
- Run `cargo test` locally before submitting PRs.

## Commit & Pull Request Guidelines
- Commits: concise imperative subject, scoped body explaining “what/why”. Group related changes.
- PRs: include a clear description, reproduction steps (if bug), and screenshots/logs when relevant. Link issues with `Fixes #<id>`.
- Ensure workspace builds and tests pass. Call out any follow‑ups explicitly.

## Security & Configuration Tips
- MQTT: configure via CLI flags. Discovery prefix defaults to `homeassistant`.
- Availability topics: shared `calaos/availability` and per‑entity `calaos/availability/<id>` are retained.
- TLS: provide `--ssl_config_dir` with PEM files for websocket TLS.

## Architecture Overview
- Event‑driven: channels in `io_context` connect inputs, rules, outputs, MQTT, and websockets.
- Home Assistant integration uses MQTT discovery; topics live under `<node_id>/...` (default `calaos`).
