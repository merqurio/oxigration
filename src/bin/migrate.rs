use oxigration::migrate;
use env_logger;
// use clap::{Arg, Command};
use tokio;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    env_logger::init();
    // let matches = Command::new("oxigration")
    //     .arg(
    //         Arg::new("base_dir")
    //         .short('b')
    //         .long("base-dir")
    //         .value_name("DIR")
    //         .default_value("../tests/schemas")
    //     )
    //     .arg(
    //         Arg::new("connection_string")
    //         .short('c')
    //         .long("connection-string")
    //         .value_name("STRING")
    //         .default_value("sqlite:///memory")
    //     )
    //     .get_matches();

    // let base_dir = matches.value_source("base_dir");
    // let connection_string = matches.value_source("connection_string");

    let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/schemas");
    if let Err(e) = migrate(base_dir.to_str().unwrap(), "sqlite:///memory").await {
        eprintln!("Error during migration: {}", e);
    } else {
        println!("Migration completed successfully");
    }
}