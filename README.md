# calaos-rs
Calaos server

MQTT + Home Assistant
- Publishes Home Assistant MQTT Discovery for inputs (binary_sensors), and outputs (switches and covers) under the `homeassistant` prefix.
- Topics:
  - State: `calaos/state/<id>` (not retained)
  - Command: `calaos/set/<id>`
  - Availability (shared): `calaos/availability` (`online`/`offline`, retained)
  - Availability (per-entity): `calaos/availability/<id>` (`online`/`offline`, retained)
  - HA status: subscribes to `homeassistant/status` and re-publishes discovery on `online`
- Payloads:
  - Switch: `ON`/`OFF`
  - Cover command: `OPEN`/`CLOSE`/`STOP`
  - Cover state: `open`/`opening`/`closed`/`closing`

Notes
- Inputs are exposed as binary_sensors; they reflect the raw input (pressed/not pressed) state.
- MQTT settings default to host `localhost:1883`, username/password empty, discovery prefix `homeassistant`, node id `calaos`.
- Environment overrides:
  - `MQTT_HOST`, `MQTT_PORT`, `MQTT_USERNAME`, `MQTT_PASSWORD`
  - `MQTT_DISCOVERY_PREFIX`, `MQTT_NODE_ID`, `MQTT_KEEP_ALIVE_SEC`
