use std::time::Duration;

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

// Configuration is now provided by the server binary via CLI arguments.
