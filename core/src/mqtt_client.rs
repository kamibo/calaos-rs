use rumqttc::{AsyncClient, LastWill, MqttOptions, QoS};
use serde_json::json;
use std::error::Error;
use std::sync::Arc;
use tracing::*;

use crate::config::io::{IoConfig, OutputKind};
use crate::config::mqtt::MqttConfig;
use crate::io_context::{BroadcastIODataRx, IOData, IODataAction, MpscIODataActionTx};
use crate::io_value::{IOAction, IOValue, ShutterState};

pub struct MqttClient {
    client: AsyncClient,
    io_config: Arc<IoConfig>,
    mqtt_config: MqttConfig,
}

impl MqttClient {
    pub async fn new(
        mqtt_config: MqttConfig,
        io_config: Arc<IoConfig>,
        tx_action: MpscIODataActionTx,
    ) -> Result<Self, Box<dyn Error>> {
        let mut mqtt_options =
            MqttOptions::new(&mqtt_config.node_id, &mqtt_config.host, mqtt_config.port);
        mqtt_options.set_keep_alive(mqtt_config.keep_alive);
        // Set LWT for Home Assistant availability
        let availability_topic = format!("{}/availability", mqtt_config.node_id);
        let will = LastWill::new(
            availability_topic.clone(),
            "offline".to_string(),
            QoS::AtLeastOnce,
            true,
        );
        mqtt_options.set_last_will(will);

        if let (Some(username), Some(password)) =
            (mqtt_config.username.as_ref(), mqtt_config.password.as_ref())
        {
            mqtt_options.set_credentials(username, password);
        }

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 100);

        // Clone values needed for the event loop
        let tx_action_clone = tx_action.clone();
        let node_id = mqtt_config.node_id.clone();
        let client_clone = client.clone();
        let io_config_clone = Arc::clone(&io_config);
        let mqtt_config_clone = mqtt_config.clone();

        // Start MQTT event loop
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(notification) => {
                        if let rumqttc::Event::Incoming(rumqttc::Packet::Publish(msg)) =
                            notification
                        {
                            let topic = msg.topic.clone();
                            let payload = String::from_utf8_lossy(&msg.payload).to_string();
                            if topic == "homeassistant/status" && payload == "online" {
                                // Re-publish discovery and set per-entity availability online
                                if let Err(e) = Self::publish_discovery_with(
                                    &client_clone,
                                    &io_config_clone,
                                    &mqtt_config_clone,
                                )
                                .await
                                {
                                    error!(
                                        "Failed to re-publish discovery after HA status: {:?}",
                                        e
                                    );
                                }
                                // Publish per-entity availability topics as online
                                for room in &io_config_clone.home.rooms {
                                    for input in &room.inputs {
                                        let t = format!(
                                            "{}/availability/{}",
                                            mqtt_config_clone.node_id, input.id
                                        );
                                        if let Err(e) = client_clone
                                            .publish(&t, QoS::AtLeastOnce, true, "online")
                                            .await
                                        {
                                            error!("Failed to publish input availability: {:?}", e);
                                        }
                                    }
                                    for output in &room.outputs {
                                        let t = format!(
                                            "{}/availability/{}",
                                            mqtt_config_clone.node_id, output.id
                                        );
                                        if let Err(e) = client_clone
                                            .publish(&t, QoS::AtLeastOnce, true, "online")
                                            .await
                                        {
                                            error!(
                                                "Failed to publish output availability: {:?}",
                                                e
                                            );
                                        }
                                    }
                                }
                            } else if let Err(e) =
                                Self::handle_command(&msg, &tx_action_clone, &node_id).await
                            {
                                error!("Failed to handle MQTT command: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("MQTT Error: {:?}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(Self {
            client,
            io_config,
            mqtt_config,
        })
    }

    async fn publish_entities_availability_online(&self) -> Result<(), Box<dyn Error>> {
        for room in &self.io_config.home.rooms {
            for input in &room.inputs {
                let topic = format!("{}/availability/{}", self.mqtt_config.node_id, input.id);
                self.client
                    .publish(&topic, QoS::AtLeastOnce, true, "online")
                    .await?;
            }
            for output in &room.outputs {
                let topic = format!("{}/availability/{}", self.mqtt_config.node_id, output.id);
                self.client
                    .publish(&topic, QoS::AtLeastOnce, true, "online")
                    .await?;
            }
        }
        Ok(())
    }

    async fn handle_command(
        msg: &rumqttc::Publish,
        tx_action: &MpscIODataActionTx,
        node_id: &str,
    ) -> Result<(), Box<dyn Error>> {
        let topic = msg.topic.clone();
        let payload = String::from_utf8_lossy(&msg.payload);

        // Expected topic format: {node_id}/set/{id}
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() != 3 || parts[0] != node_id || parts[1] != "set" {
            return Ok(());
        }

        let id = parts[2].to_string();
        let action = match payload.to_uppercase().as_str() {
            "ON" => IOAction::SetValue(IOValue::Bool(true)),
            "OFF" => IOAction::SetValue(IOValue::Bool(false)),
            "OPEN" => IOAction::SetValue(IOValue::Shutter(ShutterState::Up)),
            "CLOSE" => IOAction::SetValue(IOValue::Shutter(ShutterState::Down)),
            "STOP" => IOAction::Stop,
            "TOGGLE" => IOAction::Toggle,
            _ => return Ok(()),
        };

        tx_action.send(IODataAction::new(id, action)).await?;
        Ok(())
    }

    pub async fn start(
        &self,
        mut rx_state: BroadcastIODataRx,
        mut shutdown: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), Box<dyn Error>> {
        // Publish availability ONLINE and discovery information
        let availability_topic = format!("{}/availability", self.mqtt_config.node_id);
        self.client
            .publish(availability_topic, QoS::AtLeastOnce, true, "online")
            .await?;
        // Publish per-entity availability to retained ONLINE
        self.publish_entities_availability_online().await?;
        self.publish_discovery().await?;

        // Subscribe to command topics
        self.subscribe_to_commands().await?;

        // Handle state changes and shutdown
        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    // Proactively publish per-entity and shared offline, then exit
                    let _ = self.publish_entities_availability_offline().await;
                    break;
                }
                Ok(state) = rx_state.recv() => {
                    self.publish_state(&state).await?;
                }
            }
        }

        Ok(())
    }

    async fn publish_entities_availability_offline(&self) -> Result<(), Box<dyn Error>> {
        // Shared offline
        let shared = format!("{}/availability", self.mqtt_config.node_id);
        self.client
            .publish(&shared, QoS::AtLeastOnce, true, "offline")
            .await?;
        for room in &self.io_config.home.rooms {
            for input in &room.inputs {
                let topic = format!("{}/availability/{}", self.mqtt_config.node_id, input.id);
                self.client
                    .publish(&topic, QoS::AtLeastOnce, true, "offline")
                    .await?;
            }
            for output in &room.outputs {
                let topic = format!("{}/availability/{}", self.mqtt_config.node_id, output.id);
                self.client
                    .publish(&topic, QoS::AtLeastOnce, true, "offline")
                    .await?;
            }
        }
        Ok(())
    }

    async fn publish_discovery(&self) -> Result<(), Box<dyn Error>> {
        Self::publish_discovery_with(&self.client, &self.io_config, &self.mqtt_config).await
    }

    async fn publish_discovery_with(
        client: &AsyncClient,
        io_config: &IoConfig,
        mqtt_config: &MqttConfig,
    ) -> Result<(), Box<dyn Error>> {
        for room in &io_config.home.rooms {
            // Publish input discoveries
            for input in &room.inputs {
                let availability = json!([
                    {
                        "topic": format!("{}/availability", mqtt_config.node_id),
                        "payload_available": "online",
                        "payload_not_available": "offline"
                    },
                    {
                        "topic": format!("{}/availability/{}", mqtt_config.node_id, input.id),
                        "payload_available": "online",
                        "payload_not_available": "offline"
                    }
                ]);
                let config = json!({
                    "name": input.name,
                    "unique_id": format!("{}_{}", room.name, input.id),
                    "state_topic": format!("{}/state/{}", mqtt_config.node_id, input.id),
                    "payload_on": "ON",
                    "payload_off": "OFF",
                    "availability": availability,
                    "device": {
                        "identifiers": [&mqtt_config.node_id],
                        "name": "Calaos",
                        "model": "Calaos Controller",
                        "manufacturer": "Calaos"
                    }
                });
                let topic = format!(
                    "{}/binary_sensor/{}/{}_{}/config",
                    mqtt_config.discovery_prefix, mqtt_config.node_id, room.name, input.id
                );
                client
                    .publish(&topic, QoS::AtLeastOnce, true, serde_json::to_vec(&config)?)
                    .await?;
            }

            // Publish output discoveries
            for output in &room.outputs {
                let base = json!({
                    "name": output.name,
                    "unique_id": format!("{}_{}", room.name, output.id),
                    "state_topic": format!("{}/state/{}", mqtt_config.node_id, output.id),
                    "availability": [
                        {
                            "topic": format!("{}/availability", mqtt_config.node_id),
                            "payload_available": "online",
                            "payload_not_available": "offline"
                        },
                        {
                            "topic": format!("{}/availability/{}", mqtt_config.node_id, output.id),
                            "payload_available": "online",
                            "payload_not_available": "offline"
                        }
                    ],
                    "device": {
                        "identifiers": [&mqtt_config.node_id],
                        "name": "Calaos",
                        "model": "Calaos Controller",
                        "manufacturer": "Calaos"
                    }
                });
                let config = match output.kind {
                    OutputKind::WOShutter(_) => {
                        let mut m = base.as_object().unwrap().clone();
                        m.insert(
                            "command_topic".into(),
                            json!(format!("{}/set/{}", mqtt_config.node_id, output.id)),
                        );
                        m.insert("payload_open".into(), json!("OPEN"));
                        m.insert("payload_close".into(), json!("CLOSE"));
                        m.insert("payload_stop".into(), json!("STOP"));
                        m.insert("state_open".into(), json!("open"));
                        m.insert("state_opening".into(), json!("opening"));
                        m.insert("state_closed".into(), json!("closed"));
                        m.insert("state_closing".into(), json!("closing"));
                        m.insert("device_class".into(), json!("shutter"));
                        json!(m)
                    }
                    _ => {
                        let mut m = base.as_object().unwrap().clone();
                        m.insert(
                            "command_topic".into(),
                            json!(format!("{}/set/{}", mqtt_config.node_id, output.id)),
                        );
                        m.insert("payload_on".into(), json!("ON"));
                        m.insert("payload_off".into(), json!("OFF"));
                        m.insert("state_on".into(), json!("ON"));
                        m.insert("state_off".into(), json!("OFF"));
                        json!(m)
                    }
                };
                let topic = match output.kind {
                    OutputKind::WOShutter(_) => format!(
                        "{}/cover/{}/{}_{}/config",
                        mqtt_config.discovery_prefix, mqtt_config.node_id, room.name, output.id
                    ),
                    _ => format!(
                        "{}/switch/{}/{}_{}/config",
                        mqtt_config.discovery_prefix, mqtt_config.node_id, room.name, output.id
                    ),
                };
                client
                    .publish(&topic, QoS::AtLeastOnce, true, serde_json::to_vec(&config)?)
                    .await?;
            }
        }
        Ok(())
    }

    async fn subscribe_to_commands(&self) -> Result<(), Box<dyn Error>> {
        // Subscribe to command topic pattern: {node_id}/set/{id}
        self.client
            .subscribe(
                format!("{}/set/+", self.mqtt_config.node_id),
                QoS::AtLeastOnce,
            )
            .await?;
        // Subscribe to Home Assistant status to re-publish discovery when HA comes online
        self.client
            .subscribe("homeassistant/status", QoS::AtLeastOnce)
            .await?;
        Ok(())
    }

    async fn publish_state(&self, state: &IOData) -> Result<(), Box<dyn Error>> {
        let state_str = encode_state_string(&state.value);

        let topic = format!("{}/state/{}", self.mqtt_config.node_id, state.id);
        self.client
            .publish(topic, QoS::AtLeastOnce, false, state_str)
            .await?;
        Ok(())
    }
}

pub(crate) fn encode_state_string(value: &IOValue) -> String {
    match value {
        IOValue::Bool(b) => if *b { "ON" } else { "OFF" }.to_string(),
        IOValue::Int(i) => i.to_string(),
        IOValue::String(s) => s.clone(),
        IOValue::Shutter(s) => match s {
            ShutterState::Up => "open",
            ShutterState::Down => "closed",
            ShutterState::MovingUp => "opening",
            ShutterState::MovingDown => "closing",
        }
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_state_switch_and_cover() {
        assert_eq!(encode_state_string(&IOValue::Bool(true)), "ON");
        assert_eq!(encode_state_string(&IOValue::Bool(false)), "OFF");
        assert_eq!(encode_state_string(&IOValue::Int(42)), "42");
        assert_eq!(encode_state_string(&IOValue::String("x".into())), "x");
        assert_eq!(
            encode_state_string(&IOValue::Shutter(ShutterState::Up)),
            "open"
        );
        assert_eq!(
            encode_state_string(&IOValue::Shutter(ShutterState::Down)),
            "closed"
        );
        assert_eq!(
            encode_state_string(&IOValue::Shutter(ShutterState::MovingUp)),
            "opening"
        );
        assert_eq!(
            encode_state_string(&IOValue::Shutter(ShutterState::MovingDown)),
            "closing"
        );
    }
}
