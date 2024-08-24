use indexmap::IndexMap;
use sqlx::{query, query_scalar, AnyPool, Executor, Row};
use std::error::Error;
use std::sync::atomic::Ordering;

use crate::relational_object::DatabaseObject;
use crate::utils::{format_query_with_schema, SCHEMA_SUPPORT};

/// This function initializes the deploy log and the configuration settings in the database.
/// It performs the following steps:
///
/// 1. Creates a deploy schema if it does not already exist.
/// 2. Creates a `deploy_log` table to keep track of all the changes that have been applied to the database.
/// 3. Creates a `deploy_log_config` table to store configuration settings related to the deployment process.
/// 4. Inserts the initial configuration settings into the `deploy_log_config` table.
///
/// The `deploy_log` table is crucial for tracking which changes have been applied to the database, ensuring that
/// changes are not reapplied, and enabling rollback functionality. The `deploy_log_config` table stores settings
/// that can influence the deployment process, such as environment-specific configurations.
///
/// # Arguments
///
/// * `connection_string` - A string slice that holds the connection string to the target database.
///
/// # Returns
///
/// This function returns a `Result`:
/// * `Ok(true)` if the deploy log is successfully initialized.
/// * `Err(Box<dyn Error>)` if there is an error during the initialization process.
///
/// # Errors
///
/// This function will return an error if:
/// * There is an issue connecting to the database.
/// * There is an error executing the SQL statements to create the schema, tables, or insert the configuration settings.
pub async fn init_deploy_log(connection_string: &str) -> Result<bool, Box<dyn Error>> {
    let pool = AnyPool::connect(connection_string).await?;

    // Check if the database is SQLite
    let is_sqlite = connection_string.starts_with("sqlite");

    if !is_sqlite {
        // Check if the database supports schemas
        let supports_schemas: bool = query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.schemata WHERE schema_name = 'information_schema');"
        )
        .fetch_one(&pool)
        .await?;

        SCHEMA_SUPPORT.store(supports_schemas, Ordering::Relaxed);

        if supports_schemas {
            // Create oxigration schema if it does not exist
            pool.execute("CREATE SCHEMA IF NOT EXISTS oxigration;")
                .await?;
        }
    }

    // Create deploy_log table if it does not exist
    pool.execute(
        &*format_query_with_schema(
            "CREATE TABLE IF NOT EXISTS {schema_prefix}deploy_log (
                id INTEGER PRIMARY KEY,
                change_name TEXT NOT NULL,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .to_string(),
    )
    .await?;

    // Create deploy_log_config table if it does not exist
    let query = format_query_with_schema(
        "CREATE TABLE IF NOT EXISTS {schema_prefix}deploy_log_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
    .to_string();
    pool.execute(&*query).await?;

    Ok(true)
}

/// The function checks if the deploy log exists in the database by checking the deploy_log table and that there is content in the deploy_log table.
/// This is crucial for determining if a rollback is possible. The function follows these steps:
///
/// 1. It verifies if the `deploy_log` table exists in the database.
/// 2. It checks if there are any entries in the `deploy_log` table.
/// 3. If the `deploy_log` table exists and has entries, it indicates that rollback is possible.
/// 4. If the `deploy_log` table does not exist or has no entries, rollback is not possible.
///
/// The function returns a boolean indicating if the deploy log exists and has content.
///
/// # Arguments
///
/// * `connection_string` - A string slice that holds the connection string to the target database.
///
/// # Returns
///
/// This function returns a `Result`:
/// * `Ok(true)` if the deploy log exists and has content.
/// * `Ok(false)` if the deploy log does not exist or has no content.
/// * `Err(Box<dyn Error>)` if there is an error during the check.
///
/// # Errors
///
/// This function will return an error if:
/// * There is an issue connecting to the database.
/// * There is an error executing the query to check the `deploy_log` table.
pub async fn check_deploy_log_in_db(connection_string: &str) -> Result<bool, Box<dyn Error>> {
    let pool = AnyPool::connect(connection_string).await?;

    // Check if the deploy_log table exists
    let table_exists: bool = query_scalar(
        "SELECT EXISTS (SELECT table_name FROM information_schema.tables WHERE table_schema = 'oxigration' AND table_name = 'deploy_log');"
    )
    .fetch_one(&pool)
    .await?;

    if !table_exists {
        return Ok(false);
    }

    // Check if the deploy_log table has entries
    let log_has_entries: bool =
        query_scalar("SELECT EXISTS (SELECT 1 FROM oxigration.deploy_log LIMIT 1);")
            .fetch_one(&pool)
            .await?;

    Ok(log_has_entries)
}

/// The function reads the deploy log from the database
/// Returns an indexmap of DatabaseObject
pub async fn read_deploy_log(
    connection_string: &str,
) -> Result<IndexMap<String, DatabaseObject>, Box<dyn Error>> {
    if !check_deploy_log_in_db(connection_string).await? {
        return Err("Deploy log does not exist in the database".into());
    }

    let pool = AnyPool::connect(connection_string).await?;
    let mut deploy_log = IndexMap::new();

    let rows = query("SELECT change_name FROM oxigration.deploy_log;")
        .fetch_all(&pool)
        .await?;

    for _ in rows {
        // let change_name: String = row.try_get("change_name")?;
        // Assuming DatabaseObject can be created from change_name
        // let db_object = DatabaseObject::new(change_name.clone(), /* other required args */);
        // deploy_log.insert(change_name, db_object);
    }

    Ok(deploy_log)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::AnyPool;

    #[tokio::test]
    async fn test_init_deploy_log() -> Result<(), Box<dyn Error>> {
        // Install the default drivers
        sqlx::any::install_default_drivers();

        // Use an in-memory SQLite database for testing
        let connection_string = "postgresql://postgres@0.0.0.0/postgres";
        let pool = AnyPool::connect(connection_string).await?;

        // Initialize the deploy log
        let result = init_deploy_log(connection_string).await?;
        assert!(result, "Initialization should return true");

        // Verify the oxigration schema exists (only if not SQLite)
        if !connection_string.starts_with("sqlite") {
            let schema_exists: bool = query_scalar(
                "SELECT EXISTS (SELECT schema_name FROM information_schema.schemata WHERE schema_name = 'oxigration');"
            )
            .fetch_one(&pool)
            .await?;
            assert!(schema_exists, "oxigration schema should exist");
        }

        // Verify the deploy_log table exists
        let table_exists: bool = query_scalar(
            "SELECT EXISTS (SELECT table_name FROM information_schema.tables WHERE table_name = 'deploy_log');"
        )
        .fetch_one(&pool)
        .await?;
        assert!(table_exists, "deploy_log table should exist");

        // Verify the deploy_log_config table exists
        let config_table_exists: bool = query_scalar(
            "SELECT EXISTS (SELECT table_name FROM information_schema.tables WHERE table_name = 'deploy_log_config');"
        )
        .fetch_one(&pool)
        .await?;
        assert!(config_table_exists, "deploy_log_config table should exist");

        Ok(())
    }
}
