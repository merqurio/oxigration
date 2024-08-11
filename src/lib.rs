mod reference;
use reference::read_desired_state;
use std::collections::{HashSet,HashMap};

use pg_query::protobuf::ParseResult;

/// Represents a SQL object with its properties and dependencies.
#[derive(Debug, Clone)]
pub struct RelationalObject {
    pub schema_name: String,
    pub object_type: String,
    pub object_name: String,
    pub object_definition: ParseResult,
    pub dependencies: HashSet<String>,
    pub properties: HashMap<String, String>,
}

impl RelationalObject {
    /// Creates a new SqlObject with the given parameters.
    pub fn new(
        schema_name: String,
        object_type: String,
        object_name: String,
        object_definition: ParseResult,
        dependencies: HashSet<String>,
        properties: HashMap<String, String>,
    ) -> Self {
        RelationalObject {
            schema_name,
            object_type,
            object_name,
            object_definition,
            dependencies,
            properties
        }
    }

    /// Adds a dependency to the SqlObject.
    pub fn add_dependency(&mut self, dependency: String) {
        self.dependencies.insert(dependency);
    }

    /// Adds a property to the SqlObject.
    pub fn add_property(&mut self, key: String, value: String) {
        self.properties.insert(key, value);
    }
}

pub async fn migrate(base_dir: &str, _connection_string: &str) -> Result<(), Box<dyn std::error::Error>> {

    // Perform checks
      // Check if the base_dir exists
      // Check if the tartget DB is reachable
      // Check if the environment variables are set (DEV, TEST, PROD)
      // Check if the target DB is the correct one (DEV, TEST, PROD)
      // Check if rollback is possible (if the deploy log exists)
    // Read the changes from the source code
    let _desired_state = read_desired_state(base_dir)?;

    // Read changes from the deploy log in the target database
    // Compute the changeset between the source code and the deploy log
    // Apply changes to the target database
    // Apply changes to the deploy log
    // Disconnect from the DB

    Ok(())
}




