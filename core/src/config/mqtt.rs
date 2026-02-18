use std::time::Duration;
use std::env;

#[derive(Clone, Debug, Deserialize)]
pub struct MqttConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_discovery_prefix")]
    pub discovery_prefix: String,
    #[serde(default = "default_node_id")]
    pub node_id: String,
    #[serde(default = "default_keep_alive")]
    pub keep_alive: Duration,
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    1883
}

fn default_discovery_prefix() -> String {
    "homeassistant".to_string()
}

fn default_node_id() -> String {
    "calaos".to_string()
}

fn default_keep_alive() -> Duration {
    Duration::from_secs(30)
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            username: None,
            password: None,
            discovery_prefix: default_discovery_prefix(),
            node_id: default_node_id(),
            keep_alive: default_keep_alive(),
        }
    }
} 

impl MqttConfig {
    pub fn from_env_or_default() -> Self {
        let mut cfg = Self::default();

        if let Ok(v) = env::var("MQTT_HOST") { if !v.is_empty() { cfg.host = v; } }
        if let Ok(v) = env::var("MQTT_PORT") { if let Ok(p) = v.parse() { cfg.port = p; } }
        if let Ok(v) = env::var("MQTT_USERNAME") { if !v.is_empty() { cfg.username = Some(v); } }
        if let Ok(v) = env::var("MQTT_PASSWORD") { if !v.is_empty() { cfg.password = Some(v); } }
        if let Ok(v) = env::var("MQTT_DISCOVERY_PREFIX") { if !v.is_empty() { cfg.discovery_prefix = v; } }
        if let Ok(v) = env::var("MQTT_NODE_ID") { if !v.is_empty() { cfg.node_id = v; } }
        if let Ok(v) = env::var("MQTT_KEEP_ALIVE_SEC") {
            if let Ok(secs) = v.parse::<u64>() { cfg.keep_alive = Duration::from_secs(secs); }
        }

        cfg
    }
}
