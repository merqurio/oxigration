use clap::{Arg, Command};
use env_logger;
use oxigration::{generate, init, migrate};
use tokio;

fn build_cli() -> Command {
    Command::new("oxigration")
        .about("Oxigration: DBMS Schema Migration Manager")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("init")
                .about("Initialize the oxigration metadata to keep track of schema migrations")
                .arg(
                    Arg::new("connection")
                        .short('c')
                        .long("connection")
                        .default_value("postgresql://postgres@0.0.0.0/postgres")
                        .help("Database connection string"),
                ),
        )
        .subcommand(
            Command::new("generate")
                .about("Generate source code of the schema from the DBMS for existing database")
                .arg(
                    Arg::new("dir")
                        .short('d')
                        .long("dir")
                        .default_value("schemas/")
                        .help("Directory to store generated schemas"),
                )
                .arg(
                    Arg::new("connection")
                        .short('c')
                        .long("connection")
                        .default_value("postgresql://postgres@0.0.0.0/postgres")
                        .help("Database connection string"),
                ),
        )
        .subcommand(
            Command::new("migrate")
                .about("Migrate the DBMS by applying changes to the database based on the source code schema files")
                .arg(
                    Arg::new("dir")
                        .short('d')
                        .long("dir")
                        .default_value("schemas/")
                        .help("Directory containing schema files"),
                )
                .arg(
                    Arg::new("connection")
                        .short('c')
                        .long("connection")
                        .default_value("postgresql://postgres@0.0.0.0/postgres")
                        .help("Database connection string"),
                ),
        )
}

/// Oxigration: DBMS Schema Migration Manager
///
/// Oxigration is a tool for managing database schema migrations, where a DBMS (Database Management System) organizes and maintains the structure of data, and a schema defines the blueprint of how data is stored and accessed. This tool simplifies the process of managing and applying database schema changes across different environments.
///
/// **Features**
///
/// - Migrate the DBMS by applying SQL changes accurately
///   - Takes DBMS objects represented in files and migrates the DBMS
///   - Tracks schema version and applied migrations
/// - Generate DBMS objects source code from the DBMS
/// - Supports multiple database environments and conditional logic
/// - Schemas are directories, with each DBMS object type in its own subdirectory
///
/// **Example Directory Layout**
///
/// ```sh
/// my_schema/
/// ├── function
/// │   ├── func1.sql
/// │   └── func_with_overload.sql
/// ├── sequence
/// │   ├── regular_sequence.sql
/// ├── sp
/// │   └── sp1.sql
/// ├── table
/// │   ├── table_a_multicol_pk.sql
/// │   ├── table_b_with_fk.sql
/// ├── usertype
/// │   └── usertype1.sql
/// └── view
///     └── view1.sql
/// ```
#[tokio::main]
async fn main() {
    env_logger::init();
    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("init", sub_matches)) => {
            let connection = sub_matches
                .get_one::<String>("connection")
                .unwrap()
                .as_str();
            if let Err(e) = init(connection).await {
                eprintln!("Error during initialization: {}", e);
            } else {
                println!("Initialization completed successfully");
            }
        }
        Some(("generate", sub_matches)) => {
            let base_dir = sub_matches.get_one::<String>("dir").unwrap().as_str();
            let connection = sub_matches
                .get_one::<String>("connection")
                .unwrap()
                .as_str();
            if let Err(e) = generate(base_dir, connection).await {
                eprintln!("Error during generation: {}", e);
            } else {
                println!("Generation completed successfully");
            }
        }
        Some(("migrate", sub_matches)) => {
            let base_dir = sub_matches.get_one::<String>("dir").unwrap().as_str();
            let connection = sub_matches
                .get_one::<String>("connection")
                .unwrap()
                .as_str();
            if let Err(e) = migrate(base_dir, connection).await {
                eprintln!("Error during migration: {}", e);
            } else {
                println!("Migration completed successfully");
            }
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_init() {
        let cmd = build_cli();

        let matches = cmd.try_get_matches_from(vec![
            "oxigration",
            "init",
            "-c",
            "postgresql://test@localhost/test",
        ]);

        assert!(matches.is_ok());
        let matches = matches.unwrap();
        assert_eq!(matches.subcommand_name(), Some("init"));
        if let Some(sub_matches) = matches.subcommand_matches("init") {
            let _c = sub_matches.get_one::<String>("connection").unwrap();
            assert_eq!(
                sub_matches.get_one::<String>("connection").unwrap(),
                "postgresql://test@localhost/test"
            );
        }
    }

    #[test]
    fn test_cli_init_default_connection() {
        let cmd = build_cli();

        let matches = cmd.try_get_matches_from(vec!["oxigration", "init"]);

        assert!(matches.is_ok());
        let matches = matches.unwrap();
        assert_eq!(matches.subcommand_name(), Some("init"));
        if let Some(sub_matches) = matches.subcommand_matches("init") {
            assert_eq!(
                sub_matches.get_one::<String>("connection").unwrap(),
                "postgresql://postgres@0.0.0.0/postgres"
            );
        }
    }

    #[test]
    fn test_cli_generate() {
        let cmd = build_cli();

        let matches = cmd.try_get_matches_from(vec![
            "oxigration",
            "generate",
            "-d",
            "test_schemas/",
            "-c",
            "postgresql://test@localhost/test",
        ]);

        assert!(matches.is_ok());
        let matches = matches.unwrap();
        assert_eq!(matches.subcommand_name(), Some("generate"));
        if let Some(sub_matches) = matches.subcommand_matches("generate") {
            assert_eq!(
                sub_matches.get_one::<String>("dir").unwrap(),
                "test_schemas/"
            );
            assert_eq!(
                sub_matches.get_one::<String>("connection").unwrap(),
                "postgresql://test@localhost/test"
            );
        }
    }

    #[test]
    fn test_cli_migrate() {
        let cmd = build_cli();

        let matches = cmd.try_get_matches_from(vec![
            "oxigration",
            "migrate",
            "-d",
            "test_schemas/",
            "-c",
            "postgresql://test@localhost/test",
        ]);

        assert!(matches.is_ok());
        let matches = matches.unwrap();
        assert_eq!(matches.subcommand_name(), Some("migrate"));
        if let Some(sub_matches) = matches.subcommand_matches("migrate") {
            assert_eq!(
                sub_matches.get_one::<String>("dir").unwrap(),
                "test_schemas/"
            );
            assert_eq!(
                sub_matches.get_one::<String>("connection").unwrap(),
                "postgresql://test@localhost/test"
            );
        }
    }
}
