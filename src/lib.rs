#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate nom;

pub mod calaos_json_protocol;
pub mod calaos_protocol;
pub mod event;
pub mod io;
pub mod io_config;
pub mod io_context;
pub mod io_value;
pub mod main_server;
pub mod rules_config;
pub mod rules_engine;
pub mod websocket_server;

mod task;
