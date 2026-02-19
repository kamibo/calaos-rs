# Calaos RS

A Rust workspace with two crates:
- `core/` (library): protocol parsing, IO controllers, rules engine, MQTT client, websocket server.
- `server/` (binary): wires configs, channels, MQTT, IO controllers, and servers.

## Build & Test
- Build: `cargo build`
- Test: `cargo test`

## Run
```
cargo run -p server -- <io_config.xml> <rules_config.xml> [--ssl_config_dir <dir>] [--no_input] [--no_output]
```

Environment overrides (MQTT): `MQTT_HOST`, `MQTT_PORT`, `MQTT_USERNAME`, `MQTT_PASSWORD`, `MQTT_DISCOVERY_PREFIX`, `MQTT_NODE_ID`, `MQTT_KEEP_ALIVE_SEC`.
Optional shutdown grace: `SHUTDOWN_GRACE_MS` (default 1000 ms).

## Websocket TLS
Pass `--ssl_config_dir <dir>` to enable TLS for the websocket server.
- Certificates: looks for `fullchain.pem`, then `cert.pem`, otherwise the first `*.pem` in the directory that yields any certs.
- Private key: looks for `privkey.pem`, then `key.pem`, otherwise the first `*.pem` containing a private key.
- Key formats: prefers PKCS#8, falls back to RSA (both PEM).
- You may also pass a direct path to a single PEM file for certs or key; directory is recommended.

If no TLS directory is provided, the websocket server runs without TLS on the non‑TLS port.

## Logging
Uses `tracing` with a configured subscriber in the server binary.
