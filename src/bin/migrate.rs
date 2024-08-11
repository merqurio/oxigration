use oxigration::migrate;
use env_logger;
use clap::{Arg, Command};
use tokio;

#[tokio::main]
async fn main() {
    env_logger::init();
    let matches = Command::new("oxigration")
        .arg(
            Arg::new("dir")
            .short('d')
            .long("dir")
            .value_name("DIR")
            .default_value("schemas/")
        )
        .arg(
            Arg::new("connection")
            .short('c')
            .long("connection")
            .value_name("STRING")
            .default_value("sqlite:///memory")
        )
        .get_matches();

    let base_dir = matches.get_one::<String>("dir").unwrap().as_str();
    let connection =matches.get_one::<String>("connection").unwrap().as_str(); 
    if let Err(e) = migrate(base_dir, connection).await {
        eprintln!("Error during migration: {}", e);
    } else {
        println!("Migration completed successfully");
    }
}