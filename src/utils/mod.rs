pub mod topsort;

use std::sync::atomic::{AtomicBool, Ordering};

pub static SCHEMA_SUPPORT: AtomicBool = AtomicBool::new(false);

/// Formats a query template by replacing the `{schema_prefix}` placeholder with the appropriate schema prefix.
///
/// This function is useful for dynamically generating SQL queries that need to be compatible with databases
/// that may or may not support schemas. If schema support is enabled, the `{schema_prefix}` placeholder in the
/// query template will be replaced with the schema name (e.g., "oxigration."). If schema support is not enabled,
/// the placeholder will be replaced with an empty string.
///
/// # Arguments
///
/// * `query_template` - A string slice that holds the SQL query template containing the `{schema_prefix}` placeholder.
///
/// # Returns
///
/// This function returns a `String`:
/// * The formatted query with the `{schema_prefix}` placeholder replaced by the appropriate schema prefix.
///
/// # Example
///
/// ```
/// let query_template = "SELECT * FROM {schema_prefix}deploy_log;";
/// let formatted_query = crate::deploy_log::format_query_with_schema(query_template);
/// // If schema support is enabled, `formatted_query` will be "SELECT * FROM oxigration.deploy_log;"
/// // If schema support is not enabled, `formatted_query` will be "SELECT * FROM deploy_log;"
/// ```
pub fn format_query_with_schema(query_template: &str) -> String {
    let schema_prefix = if SCHEMA_SUPPORT.load(Ordering::Relaxed) {
        "oxigration."
    } else {
        ""
    };
    query_template.replace("{schema_prefix}", schema_prefix)
}
