#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate nom;

mod calaos_protocol;
mod io_config;
mod rules_config;

use std::env;
use std::error::Error;
use std::path::Path;

use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let file_io = match env::args().nth(1) {
        Some(file) => file,
        None => return Err("Missing file io".into()),
    };

    let file_rules = match env::args().nth(2) {
        Some(file) => file,
        None => return Err("Missing file rules".into()),
    };

    let io_config = io_config::read_from_file(Path::new(&file_io));
    let rules_config = rules_config::read_from_file(Path::new(&file_rules));

    Ok(())
}
