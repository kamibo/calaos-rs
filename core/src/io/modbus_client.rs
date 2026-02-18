use std::collections::HashMap;
use std::io::Error;
use std::net::SocketAddr;
use std::time::Instant;

use tokio_modbus::prelude::Reader;
use tokio_modbus::prelude::Writer;

// Avoid switching relay too fast
// Otherwise it could damage the relay
const MIN_DURATION_BETWEEN_WRITE: std::time::Duration = std::time::Duration::from_millis(15);

pub struct ModbusClient {
    client: tokio_modbus::client::Context,
    last_coil_update_instant: HashMap<u16, Instant>,
}

impl ModbusClient {
    pub async fn connect(socket_addr: SocketAddr) -> Result<ModbusClient, Error> {
        let client = tokio_modbus::client::tcp::connect(socket_addr).await?;

        Ok(Self {
            client,
            last_coil_update_instant: HashMap::new(),
        })
    }

    pub async fn read_discrete_inputs(
        &mut self,
        address: u16,
        quantity: u16,
    ) -> Result<Vec<bool>, Box<dyn std::error::Error>> {
        let res = self
            .client
            .read_discrete_inputs(address, quantity)
            .await??;
        Ok(res)
    }

    pub async fn read_discrete_input(
        &mut self,
        address: u16,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let values = self.client.read_discrete_inputs(address, 1).await??;
        Ok(values[0])
    }

    pub async fn write_single_coil(
        &mut self,
        address: u16,
        value: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(instant) = self.last_coil_update_instant.get(&address) {
            let delta_now = Instant::now() - *instant;
            if delta_now < MIN_DURATION_BETWEEN_WRITE {
                tokio::time::sleep(MIN_DURATION_BETWEEN_WRITE - delta_now).await;
            }
        }
        self.client.write_single_coil(address, value).await??;
        self.last_coil_update_instant
            .insert(address, Instant::now());
        Ok(())
    }
}
