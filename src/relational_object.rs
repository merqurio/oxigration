use sqlparser::ast::Statement;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct DatabaseObject {
    pub schema_name: String,
    pub object_type: String,
    pub object_name: String,
    pub object_definition: Vec<Statement>,
    pub dependencies: HashSet<String>,
    pub properties: HashMap<String, String>,
}

impl DatabaseObject {
    pub fn new(
        schema_name: String,
        object_type: String,
        object_name: String,
        object_definition: Vec<Statement>,
        dependencies: HashSet<String>,
        properties: HashMap<String, String>,
    ) -> Self {
        DatabaseObject {
            schema_name,
            object_type,
            object_name,
            object_definition,
            dependencies,
            properties,
        }
    }

    pub fn add_dependency(&mut self, dependency: String) {
        self.dependencies.insert(dependency);
    }

    pub fn add_property(&mut self, key: String, value: String) {
        self.properties.insert(key, value);
    }
}
