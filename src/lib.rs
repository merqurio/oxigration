mod deploy_log;
mod reference;
mod relational_object;
mod utils;

use deploy_log::{init_deploy_log, read_deploy_log};
use log::{error, info};
use reference::reference;
use relational_object::DatabaseObject;
use sqlx::{query_scalar, AnyPool};
use std::env;
use std::path::Path;

/// Performs pre-migration checks to ensure the base directory exists, the target database is reachable,
/// the environment variable `ENV` is set correctly, the target database matches the environment, and
/// rollback is possible by verifying the existence of the deploy log in the database.
///
/// # Arguments
///
/// * `base_dir` - A string slice that holds the path to the base directory containing the source code.
/// * `connection_string` - A string slice that holds the connection string to the target database.
///
/// # Returns
///
/// This function returns a `Result`:
/// * `Ok(())` if all checks pass.
/// * `Err(Box<dyn std::error::Error>)` if any check fails.
///
/// # Errors
///
/// This function will return an error if:
/// * The base directory does not exist.
/// * The target database is not reachable.
/// * The environment variable `ENV` is not set correctly.
/// * The target database does not match the environment.
/// * Rollback is not possible because the deploy log does not exist in the database.
async fn environment_checks(
    base_dir: &str,
    connection_string: &str,
    is_init: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // The `install_default_drivers` function is typically used to install the default SQLx drivers for database connections.
    sqlx::any::install_default_drivers();

    // Check if the target DB is reachable
    let pool = AnyPool::connect(connection_string).await?;
    let db_reachable: bool = query_scalar("SELECT TRUE;").fetch_one(&pool).await?;
    if !db_reachable {
        error!("Target database is not reachable");
        return Err("Target database is not reachable".into());
    } else {
        info!("Target database is reachable");

        // If the deploy log is being initialized, return Ok as there is no need to check anything else
        if is_init {
            return Ok(());
        }
    }

    // Check if the base_dir exists
    if !Path::new(base_dir).exists() {
        error!("Base directory does not exist: {}", base_dir);
        return Err("Base directory does not exist".into());
    } else {
        info!("Base directory exists: {}", base_dir);
    }

    // Check if the environment variables are set (DEV, TEST, PROD)
    let env = env::var("ENV").unwrap_or_else(|_| "DEV".to_string());
    if !["DEV", "TEST", "PROD", "STAGE"].contains(&env.as_str()) {
        error!("Environment variable ENV is not set correctly");
        return Err("Environment variable ENV is not set correctly".into());
    } else {
        info!("Environment variable ENV is set to: {}", env);
    }

    // Check if the target DB is the correct one (DEV, TEST, PROD)
    let db_env: String =
        query_scalar("SELECT value FROM oxigration.deploy_log_config WHERE key = 'env';")
            .fetch_one(&pool)
            .await?;

    if db_env != env {
        error!(
            "Target database environment ({}) does not match the environment variable ENV ({})",
            db_env, env
        );
        return Err(
            "Target database environment does not match the environment variable ENV".into(),
        );
    } else {
        info!(
            "Target database environment matches the environment variable ENV: {}",
            env
        );
    }

    // Check if the deploy_log table exists
    let table_exists: bool = query_scalar(
        "SELECT EXISTS (SELECT table_name FROM information_schema.tables WHERE table_schema = 'oxigration' AND table_name = 'deploy_log');"
    )
    .fetch_one(&pool)
    .await?;

    if !table_exists {
        error!("Rollback is not possible, deploy log does not exist in the database");
        return Err("Rollback is not possible, deploy log does not exist in the database".into());
    }

    // Check if the deploy_log table has entries
    let log_has_entries: bool =
        query_scalar("SELECT EXISTS (SELECT 1 FROM oxigration.deploy_log LIMIT 1);")
            .fetch_one(&pool)
            .await?;

    if !log_has_entries {
        error!("Rollback is not possible, deploy log does not exist in the database");
        // return Err("Rollback is not possible, deploy log does not exist in the database".into());
    } else {
        info!("Rollback is possible, deploy log exists in the database");
    }

    Ok(())
}

/// This function initializes the deploy log and the configuration settings in the target database.
///
/// It performs the following steps:
/// 1. Connects to the target database using the provided connection string.
/// 2. If the database supports schemas, it creates the `oxigration` schema if it does not already exist.
/// 3. Creates the `deploy_log` table if it does not already exist. This table is used to keep track of all the changes that have been applied to the database.
/// 4. Creates the `deploy_log_config` table if it does not already exist. This table is used to store configuration settings related to the deployment process.
/// 5. Inserts initial configuration settings into the `deploy_log_config` table if they do not already exist.
///
/// The `deploy_log` table is crucial for tracking which changes have been applied to the database, ensuring that changes are not reapplied, and enabling rollback functionality.
/// The `deploy_log_config` table stores settings that can influence the deployment process, such as environment-specific configurations.
///
/// # Arguments
///
/// * `base_dir` - A string slice that holds the path to the base directory containing the source code.
/// * `connection_string` - A string slice that holds the connection string to the target database.
///
/// # Returns
///
/// This function returns a `Result`:
/// * `Ok(())` if the initialization is successful.
/// * `Err(Box<dyn std::error::Error>)` if any error occurs during the initialization process.
///
/// # Errors
///
/// This function will return an error if:
/// * There is an issue connecting to the database.
/// * There is an error executing the SQL statements to create the schema, tables, or insert the configuration settings.
/// * The base directory does not exist.
/// * The target database is not reachable.
/// * The environment variable `ENV` is not set correctly.
/// * The target database does not match the environment.
/// * Rollback is not possible because the deploy log does not exist in the database.
pub async fn init(connection_string: &str) -> Result<(), Box<dyn std::error::Error>> {
    environment_checks("", connection_string, true).await?;
    init_deploy_log(connection_string).await?;
    Ok(())
}

/// Migrates the database schema based on the source code in the specified base directory.
///
/// This function performs the following steps:
/// 1. Pre-migration checks:
///    - Verifies if the base directory exists.
///    - Checks if the target database is reachable.
///    - Ensures the environment variable `ENV` is set correctly (DEV, TEST, PROD).
///    - Confirms that the target database matches the environment.
///    - Checks if rollback is possible by verifying the existence of the deploy log in the database.
/// 2. Reads and processes the desired schema and changes from the source code in the base directory.
/// 3. Reads changes from the deploy log in the target database.
/// 4. Computes the changeset between the source code and the deploy log.
/// 5. Applies changes to the target database.
/// 6. Updates the deploy log to reflect the new state of the environment.
/// 7. Disconnects from the database.
///
/// # Arguments
///
/// * `base_dir` - A string slice that holds the path to the base directory containing the source code.
/// * `_connection_string` - A string slice that holds the connection string to the target database.
///
/// # Returns
///
/// This function returns a `Result`:
/// * `Ok(())` if the migration is successful.
/// * `Err(Box<dyn std::error::Error>)` if any error occurs during the migration process.
///
/// # Errors
///
/// This function will return an error if:
/// * The pre-migration checks fails.
/// * Any other error occurs during the migration process.
pub async fn migrate(
    base_dir: &str,
    connection_string: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Pre-migration checks
    environment_checks(base_dir, connection_string, false).await?;

    // Step 0: Read and process the desired schema and changes from the source code in base_dir.
    // This step involves parsing the SQL files, processing them, and storing the information in memory. It parses the SQL inside each file and builds a graph representation of each database object, its modifications over time, and other dependencies.
    // The information from the AST tree is used to build a graph where all the other database objects that have a dependency on that object are stated with a relationship.
    // TODO: With table CREATE statements, it rewrites the initial schema based on all the ALTERS that the table might have along all the files, creating a new CREATE statement that includes all the changes.
    let _reference_source_code = reference(base_dir)?;

    // Step 1: Read changes from the deploy log in the target database
    // This step involves reading the deploy log to understand the current state of the environment.
    let _deploy_log = read_deploy_log(connection_string).await?;

    // Step 2: Compute the changeset between the source code and the deploy log
    // This step compares the changes in the source code with the entries in the deploy log.
    // let changeset = compute_changeset(&_reference_source_code, &deploy_log)?;

    // Step 3: Apply changes to the target database
    // This step involves executing the necessary SQL commands or other database modifications.
    // apply_changes_to_db(&changeset, _connection_string).await?;

    // Step 4: Apply changes to the deploy log
    // After successfully applying the changes, update the deploy log to reflect the new state of the environment.
    // update_deploy_log(&changeset, _connection_string).await?;

    // Step 5: Disconnect from the DB
    // Ensure that the database connection is properly closed.
    // disconnect_from_db(_connection_string).await?;

    Ok(())
}

/// This function generates the source code for the schema from the target database and stores it in the specified base directory.
///
/// # Arguments
///
/// * `base_dir` - A string slice that holds the path to the base directory where the generated source code will be stored.
/// * `_connection_string` - A string slice that holds the connection string to the target database.
///
/// # Returns
///
/// This function returns a `Result` type, which is:
///
/// * `Ok(())` if the operation was successful.
/// * `Err` if there was an error during the operation.
///
/// # Steps
///
/// 1. Read the schema from the target database.
/// 2. Generate the source code for the schema.
/// 3. Store the generated source code in the specified base directory.
pub async fn generate(
    base_dir: &str,
    connection_string: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read the schema from the target database
    // Generate the source code for the schema
    // Store the source code in the base_dir
    environment_checks(base_dir, connection_string, false).await?;
    Ok(())
}
