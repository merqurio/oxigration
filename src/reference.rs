use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::Read;
use walkdir::WalkDir;
use indexmap::IndexMap;

use crate::RelationalObject;

/// Reads and processes a directory containing multiple subdirectories, each representing a type of
/// database object.
///
/// The provided directory should have the following structure:
///
/// ```
/// ├── db_schema_name_1
/// │   ├── object_type_1
/// │   │   ├── object_1_1.sql
/// │   │   ├── object_1_2.sql
/// │   │   └── ...
/// │   └── object_type_2
/// │       ├── object_2_1.sql
/// │       ├── object_2_2.sql
/// │       └── ...
/// ├── db_schema_name_2
/// │   ├── ...
/// └── ...
/// ```
///
/// Each subdirectory corresponds to a different DBMS schema and inside a different directory for
/// every type of database object (e.g., tables, views, functions, etc.). Inside each subdirectory,
/// the SQL files represent instances of the respective database object.
///
/// The function reads the SQL files, processes them, and stores the information in memory. It
/// parses the SQL inside each file and builds a graph representation of each database object, it's
/// modifications over time and other dependencies. To do so, the information from the AST tree is
/// used to build a graph where all the other database objects that have a dependency in that
/// object are stated with a relationship.
///
/// With table CREATE statements, it rewrites the initial schema based on all the ALTERS that the
/// table might have along all the file, creating a new CREATE statement that includes all the
/// changes.
///
/// # Examples
///
/// ```
/// let base_dir = "/path/to/migrations";
/// let object_info = read_desired_state(base_dir)?;
/// ```
///
/// # Arguments
///
/// * `base_dir` - A string slice that holds the base directory path.
///
/// # Errors
///
/// Returns a Box<dyn Error>:
///
/// * If the file cannot be opened or read.
/// * If the file contains invalid UTF-8 data.
///
/// # Examples
///
/// ```
/// let result = read_desired_state("/path/to/dir");
/// match result {
///     Ok(desired_state) => {
///         // Do something with the HashSet
///     },
///     Err(e) => {
///         eprintln!("Error: {}", e);
///     }
/// }
/// ```

pub fn read_desired_state(base_dir: &str) -> Result<HashMap<String, RelationalObject>, Box<dyn Error>> {
    let mut object_info: HashMap<String, RelationalObject> = HashMap::new();

    log::debug!("Reading desired state from {}", base_dir);
    for entry in WalkDir::new(base_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().map_or(false, |ext| ext == "sql") {
            let file_path = entry.path();
            let schema_name = file_path.parent().and_then(|p| p.parent()).and_then(|p| p.file_name())
                .and_then(|n| n.to_str()).ok_or("Invalid schema directory")?;
            let object_type = file_path.parent().and_then(|p| p.file_name())
                .and_then(|n| n.to_str()).ok_or("Invalid object type directory")?;
            
            let mut file = File::open(file_path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            if object_type.to_lowercase() == "table" {
                let parsed_stmts = 
                    parse_change_stmts(&contents, "//// CHANGE", "GO", "name");
                for (_, stmt) in parsed_stmts {
                    if let Ok(parsed_content) = pg_query::parse(&stmt.value.to_string()) {
                        let object_name = file_path.file_stem().and_then(|n| n.to_str())
                            .ok_or("Invalid object name")?;
                        let key = format!("{}.{}.{}.{}", schema_name, object_type, object_name, stmt.name);
                        let relational_object = RelationalObject::new(
                            schema_name.to_string(),
                            object_type.to_string(),
                            key.clone(),
                            parsed_content.protobuf,
                            stmt.dependencies,
                            stmt.properties
                        );
                        object_info.insert(key, relational_object);
                    }
                }
            } else {
                if let Ok(parsed_content) = pg_query::parse(&contents) {
                    let object_name = file_path.file_stem().and_then(|n| n.to_str())
                        .ok_or("Invalid object name")?;
                    let key = format!("{}.{}.{}.{}", schema_name, object_type, object_name, "root");
                    let relational_object = RelationalObject::new(
                        schema_name.to_string(),
                        object_type.to_string(),
                        key.clone(),
                            parsed_content.protobuf,
                           HashSet::new(), 
                            HashMap::new() 
                    );
                    object_info.insert(key, relational_object);
                }
            }
        }
    }
    Ok(object_info)
}


#[derive(Debug)]
struct Stmt {
    name: String,
    value: String,
    dependencies: HashSet<String>,
    properties: HashMap<String, String>

}

impl Stmt {
    /// Creates a new SqlObject with the given parameters.
    pub fn new(
        name: String,
        value: String,
        dependencies: HashSet<String>,
        properties: HashMap<String, String>,
    ) -> Self {
        Stmt {
            name,
            value,
            dependencies,
            properties
        }
    }
}
/// Parses a string containing multiple statements delimited by start and end delimiters,
/// and returns the text between the delimeters together with the attributes defined in the start_delimetere.
///
/// # Arguments
/// * `content` - The input string containing the statements.
/// * `start_delimiter` - The delimiter marking the start of a statement.
/// * `end_delimiter` - The delimiter marking the end of a statement.
/// * `key` - The attribute defining the name of the change.
///
/// # Returns
/// A list of object with the statement name, the text between the delimiters as value and the attributes defined in the start_delimiter as list of key values.
/// [{name, value, attributes: {key: value}}]
///
/// # Examples
/// ```
/// let content = "//// CHANGE name=statement1 depends=statement2\nCREATE TABLE table1 (id INT);\nGO\n//// CHANGE name=statement2\nCREATE TABLE table2 (id INT);\nGO\n";
/// let parsed_stmts = parse_change_stmts(content, "//// CHANGE", "GO", "name");
/// assert_eq!(parsed_stmts.len(), 2);
/// assert!(parsed_stmts.contains_key("statement1"));
/// assert!(parsed_stmts.contains_key("statement2"));
/// ```
fn parse_change_stmts(content: &str, start_delimiter: &str, end_delimiter: &str, key: &str) -> IndexMap<String, Stmt> {
    let mut result: IndexMap<String, Stmt> = IndexMap::new();
    let mut dependencies: HashSet<String> = HashSet::new();
    let mut current_name = String::new();
    let mut value = String::new();
    let mut properties = HashMap::new();
    let mut in_statement = false;

    for line in content.lines() {
        if line.trim().starts_with(start_delimiter) {
            in_statement = true;
            properties = line.trim_start_matches(start_delimiter)
                .split_whitespace()
                .filter_map(|attr| {
                    let mut parts = attr.split('=');
                    Some((parts.next()?.to_string(), parts.next()?.to_string()))
                })
                .collect();
            current_name = properties.get(key).cloned().unwrap_or_default();
        } else if line.trim() == end_delimiter {
            if in_statement {
                result.insert(current_name.clone(), 
                    Stmt::new(
                        current_name.clone(),
                        value.trim().to_string(),
                        dependencies.clone(),
                        properties.clone(),
                ));
                dependencies.insert(current_name.clone());
                current_name.clear();
                value.clear();
                properties.clear();
                in_statement = false;
            }
        } else if in_statement {
            value.push_str(line);
            value.push('\n');
        }
    }

    result
}