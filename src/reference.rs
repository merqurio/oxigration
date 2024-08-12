use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use walkdir::WalkDir;
use indexmap::IndexMap;
use petgraph::graphmap::DiGraphMap;
use petgraph::algo::toposort;
use crate::RelationalObject;
use pg_query::NodeEnum;

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
pub fn read_desired_state(base_dir: &str) -> Result<IndexMap<String, RelationalObject>, Box<dyn Error>> {
    let mut object_info: IndexMap<String, RelationalObject> = IndexMap::new();

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
                let parsed_stmts = parse_change_stmts(&contents, "//// CHANGE", "GO", "name");
                for (_, stmt) in parsed_stmts {
                    if let Ok(relational_object) = process_sql_file(file_path, schema_name, object_type, &contents, Some(&stmt)) {
                        object_info.insert(relational_object.object_name.clone(), relational_object);
                    }
                }
            } else {
                if let Ok(relational_object) = process_sql_file(file_path, schema_name, object_type, &contents, None) {
                    object_info.insert(relational_object.object_name.clone(), relational_object);
                }
            }
        }
    }
    let ordered_object_info = determine_execution_order(&object_info)?;
    Ok(ordered_object_info)
}

fn process_sql_file(file_path: &Path, schema_name: &str, object_type: &str, contents: &str, stmt: Option<&Stmt>) -> Result<RelationalObject, Box<dyn Error>> {
    let parsed_content = pg_query::parse(&stmt.map_or(contents, |s| &s.value))?;
    let first_object = parsed_content.protobuf.stmts.first().ok_or("No objects found in parsed content")?;
    let node = first_object.stmt.as_ref().ok_or("No statement found in first object")?;
    
    if let Some(range_var) = match &node.node {
        Some(NodeEnum::RangeVar(range_var)) => Some(range_var),
        Some(NodeEnum::CreateStmt(create_stmt)) => create_stmt.relation.as_ref(),
        Some(NodeEnum::AlterTableStmt(alter_stmt)) => alter_stmt.relation.as_ref(),
        Some(NodeEnum::DeleteStmt(delete_stmt)) => delete_stmt.relation.as_ref(),
        _ => None,
    } {
        let object_name = file_path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| range_var.relname.clone());
        if object_name != range_var.relname {
            log::warn!("Object name '{}' in file does not match name '{}' in SQL", object_name, range_var.relname);
        }
        let schema_name = if range_var.schemaname.is_empty() {
            schema_name.to_string()
        } else {
            range_var.schemaname.clone()
        };
        
        let key = match stmt {
            Some(stmt) => format!("{}.{}.{}.{}", schema_name, object_type, object_name, stmt.change_name),
            None => format!("{}.{}.{}.{}", schema_name, object_type, object_name, "root"),
        };

        let dependencies = stmt.map_or_else(HashSet::new, |s| s.dependencies.clone());
        let properties = stmt.map_or_else(HashMap::new, |s| s.properties.clone());

        Ok(RelationalObject::new(
            schema_name.to_string(),
            object_type.to_string(),
            key,
            parsed_content.protobuf,
            dependencies,
            properties,
        ))
    } else {
        return Err("No RangeVar found in node".into());
    }
}

#[derive(Debug)]
/// Represents a statement with associated metadata.
///
/// This struct encapsulates a named statement along with its value,
/// dependencies, and additional properties.
struct Stmt {
    /// The name of the statement.
    change_name: String,
    /// The actual content or value of the statement.
    value: String,
    /// A set of dependencies for this statement.
    dependencies: HashSet<String>,
    /// Additional properties associated with the statement.
    properties: HashMap<String, String>
}

/// Provides methods for creating and manipulating `Stmt` instances.
impl Stmt {
    /// Creates a new SqlObject with the given parameters.
    pub fn new(
        change_name: String,
        value: String,
        dependencies: HashSet<String>,
        properties: HashMap<String, String>,
    ) -> Self {
        Stmt {
            change_name,
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

fn determine_execution_order(
    object_info: &IndexMap<String, RelationalObject>
) -> Result<IndexMap<String, RelationalObject>, Box<dyn std::error::Error>> {
    // Create a directed graph
    let mut graph = DiGraphMap::new();

    // Add nodes and edges based on dependencies
    for (key, obj) in object_info {
        graph.add_node(key.as_str());
        for dep in &obj.dependencies {
            graph.add_edge(dep.as_str(), key.as_str(), ());
        }
    }

    // Perform topological sort to determine execution order
    let order = toposort(&graph, None)
        .map_err(|_| "Cycle detected in dependencies")?;

    // Convert the order to a vector of strings
    let execution_order: Vec<String> = order.into_iter().map(|s| s.to_string()).collect();

    let mut ordered_object_info = IndexMap::new();
    for key in execution_order {
        if let Some(obj) = object_info.get(&key) {
            ordered_object_info.insert(key.clone(), obj.clone());
        }
    }
    Ok(ordered_object_info)
}