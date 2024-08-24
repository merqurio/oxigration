use crate::RelationalObject;
use crate::utils::topsort::topo_sort;
use indexmap::IndexMap;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use sqlparser::ast::{Visit, Visitor};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use walkdir::WalkDir;

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
pub fn read_desired_state(
    base_dir: &str,
) -> Result<IndexMap<String, RelationalObject>, Box<dyn Error>> {
    let mut object_info: IndexMap<String, RelationalObject> = IndexMap::new();

    log::debug!("Reading desired state from {}", base_dir);
    for entry in WalkDir::new(base_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().map_or(false, |ext| ext == "sql")
        {
            let file_path = entry.path();
            let schema_name = file_path
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .ok_or("Invalid schema directory")?;
            let object_type = file_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .ok_or("Invalid object type directory")?;

            let mut file = File::open(file_path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            let parsed_stmts = parse_change_stmts(&contents, "//// CHANGE", "GO", "name");
            for (_, stmt) in parsed_stmts {
                if let Ok(relational_object) = build_relational_object(
                    file_path,
                    schema_name,
                    object_type,
                    &contents,
                    Some(&stmt),
                ) {
                    object_info.insert(relational_object.object_name.clone(), relational_object);
                }
            }
        }
    }
    let ordered_object_info = determine_execution_order(&object_info)?;
    Ok(ordered_object_info)
}

/// Builds a `RelationalObject` from the given parameters.
///
/// This function parses the SQL content and constructs a `RelationalObject`
/// with the provided schema name, object type, and optional statement metadata.
///
/// # Arguments
///
/// * `file_path` - The path to the SQL file.
/// * `schema_name` - The name of the schema.
/// * `object_type` - The type of the database object (e.g., table, view).
/// * `contents` - The SQL content of the file.
/// * `stmt` - An optional statement with metadata.
///
/// # Returns
///
/// A `Result` containing the constructed `RelationalObject` or an error.
///
/// # Errors
///
/// Returns an error if the SQL content cannot be parsed or if required information is missing.
fn build_relational_object(
    file_path: &Path,
    schema_name: &str,
    object_type: &str,
    contents: &str,
    stmt: Option<&Stmt>,
) -> Result<RelationalObject, Box<dyn Error>> {
    let dialect = GenericDialect {};
    let parsed_content = Parser::parse_sql(&dialect, &stmt.map_or(contents, |s| &s.value))?;
    let first_object = parsed_content
        .first()
        .ok_or("No objects found in parsed content")?;

    let mut visitor = SqlVisitor::new();
    visitor.visit_statement(first_object);

    let object_name = file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| visitor.object_name.clone());
    if object_name != visitor.object_name {
        log::warn!(
            "Object name '{}' in file does not match name '{}' in SQL",
            object_name,
            visitor.object_name
        );
    }
    let schema_name = if visitor.schema_name.is_empty() {
        schema_name.to_string()
    } else {
        visitor.schema_name.clone()
    };

    let key = match stmt {
        Some(stmt) => format!(
            "{}.{}.{}.{}",
            schema_name, object_type, object_name, stmt.change_name
        ),
        None => format!("{}.{}.{}.{}", schema_name, object_type, object_name, "root"),
    };

    let dependencies = stmt.map_or_else(HashSet::new, |s| s.dependencies.clone());
    let properties = stmt.map_or_else(HashMap::new, |s| s.properties.clone());

    Ok(RelationalObject::new(
        schema_name.to_string(),
        object_type.to_string(),
        key,
        parsed_content,
        dependencies,
        properties,
    ))
}

struct SqlVisitor {
    object_name: String,
    schema_name: String,
}

impl SqlVisitor {
    fn new() -> Self {
        SqlVisitor {
            object_name: String::new(),
            schema_name: String::new(),
        }
    }

    fn visit_statement(&mut self, stmt: &sqlparser::ast::Statement) {
        match stmt {
            sqlparser::ast::Statement::CreateTable(stmt) => {
                self.visit_object_name(&stmt.name);
            }
            sqlparser::ast::Statement::CreateView { name, .. } => {
                self.visit_object_name(name);
            }
            sqlparser::ast::Statement::CreateFunction { name, .. } => {
                self.visit_object_name(name);
            }
            sqlparser::ast::Statement::CreateProcedure { name, .. } => {
                self.visit_object_name(name);
            }
            sqlparser::ast::Statement::CreateIndex(stmt) => {
                if let Some(name) = &stmt.name {
                    self.visit_object_name(name);
                }
            }
            sqlparser::ast::Statement::CreateSequence { name, .. } => {
                self.visit_object_name(name);
            }
            _ => {}
        }
    }

    fn visit_object_name(&mut self, name: &sqlparser::ast::ObjectName) {
        self.object_name = name.to_string();
    }

    fn visit_schema_name(&mut self, name: &sqlparser::ast::ObjectName) {
        self.schema_name = name.to_string();
    }
}

/// Represents a statement with associated metadata.
///
/// This struct encapsulates a named statement along with its value,
/// dependencies, and additional properties.
#[derive(Debug)]
struct Stmt {
    /// The name of the statement.
    change_name: String,
    /// The actual content or value of the statement.
    value: String,
    /// A set of dependencies for this statement.
    dependencies: HashSet<String>,
    /// Additional properties associated with the statement.
    properties: HashMap<String, String>,
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
            properties,
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
fn parse_change_stmts(
    content: &str,
    start_delimiter: &str,
    end_delimiter: &str,
    key: &str,
) -> IndexMap<String, Stmt> {
    let mut result: IndexMap<String, Stmt> = IndexMap::new();
    let mut dependencies: HashSet<String> = HashSet::new();
    let mut current_name = String::new();
    let mut value = String::new();
    let mut properties = HashMap::new();
    let mut in_statement = false;
    let mut root_counter = 0;

    for line in content.lines() {
        if line.trim().starts_with(start_delimiter) {
            in_statement = true;
            properties = line
                .trim_start_matches(start_delimiter)
                .split_whitespace()
                .filter_map(|attr| {
                    let mut parts = attr.split('=');
                    Some((parts.next()?.to_string(), parts.next()?.to_string()))
                })
                .collect();
            current_name = properties.get(key).cloned().unwrap_or_default();
        } else if line.trim() == end_delimiter {
            if in_statement {
                result.insert(
                    current_name.clone(),
                    Stmt::new(
                        current_name.clone(),
                        value.trim().to_string(),
                        dependencies.clone(),
                        properties.clone(),
                    ),
                );
                dependencies.insert(current_name.clone());
                current_name.clear();
                value.clear();
                properties.clear();
                in_statement = false;
            } else {
                let root_name = format!("root{}", root_counter);
                root_counter += 1;
                result.insert(
                    root_name.clone(),
                    Stmt::new(
                        root_name.clone(),
                        value.trim().to_string(),
                        dependencies.clone(),
                        properties.clone(),
                    ),
                );
                dependencies.insert(root_name.clone());
                value.clear();
            }
        } else if in_statement {
            value.push_str(line);
            value.push('\n');
        } else {
            value.push_str(line);
            value.push('\n');
        }
    }

    if !value.trim().is_empty() {
        let root_name = format!("root{}", root_counter);
        result.insert(
            root_name.clone(),
            Stmt::new(
                root_name,
                value.trim().to_string(),
                dependencies,
                properties,
            ),
        );
    }

    result
}


/// Determines the execution order of relational objects based on their dependencies.
///
/// This function takes an `IndexMap` of relational objects, where each object has a set of dependencies.
/// It constructs a directed graph from these dependencies and performs a topological sort to determine
/// the order in which the objects should be executed. If a cycle is detected in the dependencies, an error is returned.
///
/// TODO: This implementation does not handle references to objects inside the SQL content of the objects.
///
/// # Arguments
/// * `object_info` - An `IndexMap` where the keys are object names and the values are `RelationalObject` instances.
///
/// # Returns
/// A `Result` containing an `IndexMap` of the objects in the determined execution order, or an error if a cycle is detected.
///
/// # Errors
/// Returns an error if a cycle is detected in the dependencies.
fn determine_execution_order(
    object_info: &IndexMap<String, RelationalObject>,
) -> Result<IndexMap<String, RelationalObject>, Box<dyn std::error::Error>> {
    // Create a list of edges based on dependencies
    let mut edges = Vec::new();

    for (key, obj) in object_info {
        for dep in &obj.dependencies {
            edges.push((dep.as_str(), key.as_str()));
        }
    }

    // Perform topological sort to determine execution order
    let order = topo_sort(&edges)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_change_stmts_with_delimiters() {
        let content = "//// CHANGE name=statement1 depends=statement2\nCREATE TABLE table1 (id INT);\nGO\n//// CHANGE name=statement2\nCREATE TABLE table2 (id INT);\nGO\n";
        let parsed_stmts = parse_change_stmts(content, "//// CHANGE", "GO", "name");
        assert_eq!(parsed_stmts.len(), 2);
        assert!(parsed_stmts.contains_key("statement1"));
        assert!(parsed_stmts.contains_key("statement2"));
    }

    #[test]
    fn test_parse_change_stmts_without_start_delimiter() {
        let content = "CREATE TABLE table1 (id INT);\nGO\nCREATE TABLE table2 (id INT);\nGO\n";
        let parsed_stmts = parse_change_stmts(content, "//// CHANGE", "GO", "name");
        assert_eq!(parsed_stmts.len(), 2);
        assert!(parsed_stmts.contains_key("root0"));
        assert!(parsed_stmts.contains_key("root1"));
    }

    #[test]
    fn test_parse_change_stmts_without_delimiters() {
        let content = "CREATE TABLE table1 (id INT);";
        let parsed_stmts = parse_change_stmts(content, "//// CHANGE", "GO", "name");
        assert_eq!(parsed_stmts.len(), 1);
        assert!(parsed_stmts.contains_key("root0"));
    }

    #[test]
    fn test_parse_change_stmts_without_end_delimiters_and_one_statements() {
        let content = "CREATE PROCEDURE sp1() LANGUAGE plpgsql AS $$ DECLARE val INTEGER; END $$; \n\nGO";
        let parsed_stmts = parse_change_stmts(content, "//// CHANGE", "GO", "name");
        assert_eq!(parsed_stmts.len(), 1);
        assert!(parsed_stmts.contains_key("root0"));
    }

    #[test]
    fn test_parse_change_stmts_without_end_delimiters_and_multiple_statements() {
        let content = "CREATE PROCEDURE sp1() LANGUAGE plpgsql AS $$ DECLARE val INTEGER; END $$; \n\nGO\nCREATE PROCEDURE sp1() LANGUAGE plpgsql AS $$ DECLARE val INTEGER; END $$; \n\nGO";
        let parsed_stmts = parse_change_stmts(content, "//// CHANGE", "GO", "name");
        assert_eq!(parsed_stmts.len(), 2);
        assert!(parsed_stmts.contains_key("root0"));
        assert!(parsed_stmts.contains_key("root1"));
    }
}