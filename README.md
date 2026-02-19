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

Server flags (MQTT):
- `--mqtt-host` (default `localhost`)
- `--mqtt-port` (default `1883`)
- `--mqtt-username` (optional)
- `--mqtt-password` (optional)
- `--mqtt-discovery-prefix` (default `homeassistant`)
- `--mqtt-node-id` (default `calaos`)
- `--mqtt-keep-alive-sec` (default `30`)

Optional shutdown grace: env `SHUTDOWN_GRACE_MS` (default 1000 ms).

## Websocket TLS
Pass `--ssl_config_dir <dir>` to enable TLS for the websocket server.
- Certificates: looks for `fullchain.pem`, then `cert.pem`, otherwise the first `*.pem` in the directory that yields any certs.
- Private key: looks for `privkey.pem`, then `key.pem`, otherwise the first `*.pem` containing a private key.
- Key formats: prefers PKCS#8, falls back to RSA (both PEM).
- You may also pass a direct path to a single PEM file for certs or key; directory is recommended.

If no TLS directory is provided, the websocket server runs without TLS on the non‑TLS port.

### Examples

Certbot layout (recommended):

```
tls/
├── fullchain.pem   # certificate chain from Certbot
└── privkey.pem     # private key from Certbot
```

Run with:

```
cargo run -p server -- io.xml rules.xml --ssl_config_dir tls/
```

Self‑signed (development):

```
mkdir -p tls && \
  openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout tls/key.pem -out tls/cert.pem -days 365 \
    -subj "/CN=localhost"

cargo run -p server -- io.xml rules.xml --ssl_config_dir tls/
```

Notes:
- File names are auto‑discovered. If your files use different names, ensure they have `.pem` extension or pass a direct file path.
- Key parsing prefers PKCS#8 (`pkcs8_private_keys`) and falls back to RSA (`rsa_private_keys`).
- Ensure file permissions restrict private key access (e.g., `chmod 600`).

## Logging
Uses `tracing` with a configured subscriber in the server binary.
