use crate::utils::topsort::topo_sort;
use core::ops::ControlFlow;
use indexmap::IndexMap;
use sqlparser::ast::{ObjectName, Statement, Visitor};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use walkdir::WalkDir;

/// Represents a database object with associated metadata.
///
/// This struct encapsulates a named database object along with its value,
/// dependencies, and additional properties.
#[derive(Debug, Clone)]
pub struct DatabaseObject {
    /// The name of the database object.
    pub change_name: String,
    /// The actual content or value of the database object.
    pub value: String,
    /// A set of dependencies for this database object.
    pub dependencies: HashSet<String>,
    /// Additional properties associated with the database object.
    pub _properties: HashMap<String, String>,
    /// The parsed SQL content of the database object.
    pub parsed_content: Option<Statement>,
}
impl DatabaseObject {
    /// Creates a new DatabaseObject with the given parameters.
    pub fn new(
        change_name: String,
        value: String,
        mut dependencies: HashSet<String>,
        properties: HashMap<String, String>,
        parsed_content: Option<Statement>,
    ) -> Self {
        // Check if properties contain a "depends" key and add its value to dependencies
        if let Some(depends) = properties.get("depends") {
            for dep in depends.split(',') {
                dependencies.insert(dep.trim().to_string());
            }
        }

        DatabaseObject {
            change_name,
            value,
            dependencies,
            _properties: properties,
            parsed_content,
        }
    }
}

/// Visitor implementation for SQL statements.
struct SqlVisitor {
    object_name: String,
    schema_name: String,
    database_name: String,
}
impl SqlVisitor {
    fn new() -> Self {
        SqlVisitor {
            object_name: String::new(),
            schema_name: String::new(),
            database_name: String::new(),
        }
    }

    fn visit_object_name(&mut self, name: &ObjectName) {
        match name.0.len() {
            1 => {
                self.object_name = name.0[0].value.clone();
            }
            2 => {
                self.schema_name = name.0[0].value.clone();
                self.object_name = name.0[1].value.clone();
            }
            3 => {
                self.database_name = name.0[0].value.clone();
                self.schema_name = name.0[1].value.clone();
                self.object_name = name.0[2].value.clone();
            }
            _ => {}
        }
    }
}
impl Visitor for SqlVisitor {
    type Break = ();

    fn pre_visit_statement(&mut self, stmt: &Statement) -> ControlFlow<Self::Break> {
        match stmt {
            Statement::CreateTable(stmt) => {
                self.visit_object_name(&stmt.name);
            }
            Statement::CreateView { name, .. } => {
                self.visit_object_name(name);
            }
            Statement::CreateFunction { name, .. } => {
                self.visit_object_name(name);
            }
            Statement::CreateProcedure { name, .. } => {
                self.visit_object_name(name);
            }
            Statement::CreateSequence { name, .. } => {
                self.visit_object_name(name);
            }
            Statement::CreateIndex(stmt) => {
                if let Some(name) = &stmt.name {
                    self.visit_object_name(name);
                }
            }
            Statement::AlterTable { name, .. } => {
                self.visit_object_name(name);
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

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
pub fn read_source_code(
    base_dir: &str,
) -> Result<IndexMap<String, DatabaseObject>, Box<dyn Error>> {
    let mut object_info: IndexMap<String, DatabaseObject> = IndexMap::new();

    log::debug!("Reading desired state from {}", base_dir);
    // Traverse the directory structure
    for entry in WalkDir::new(base_dir).into_iter().filter_map(|e| e.ok()) {
        // Check if the entry is a file with a .sql extension
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

            // Open the SQL file
            let mut file = File::open(file_path)?;
            // Read the contents of the SQL file
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            // Parse the SQL statements in the file
            let parsed_stmts = parse_change_stmts(&contents, "//// CHANGE", "GO", "name");
            // Iterate over the parsed statements
            for (_, mut stmt) in parsed_stmts {
                // Build a relational object from the parsed statement
                match relational_object_conformance(file_path, schema_name, object_type, &mut stmt)
                {
                    Ok(_) => {
                        object_info.insert(stmt.change_name.clone(), stmt);
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
    if object_info.is_empty() {
        return Err("No database objects found".into());
    }
    // Determine the execution order of the database objects
    let ordered_object_info = determine_execution_order(&object_info)?;
    Ok(ordered_object_info)
}

/// Updates a `DatabaseObject` with the given parameters.
///
/// This function takes several parameters including the file path, schema name, object type,
/// contents of the SQL file, and a mutable reference to a `DatabaseObject`. It parses the SQL
/// content to extract the first SQL object and uses a visitor to traverse the SQL statement
/// and gather necessary information such as the object name and schema name.
///
/// The function then constructs a key for the `DatabaseObject` based on the schema name,
/// object type, file name, and change name. It also updates the dependencies and properties
/// of the `DatabaseObject` accordingly.
///
/// If the object name extracted from the SQL content does not match the file name, an error
/// is returned. Otherwise, the existing `DatabaseObject` is updated with the new information.
///
/// # Arguments
///
/// * `file_path` - A reference to the path of the SQL file.
/// * `schema_name` - A string slice representing the schema name.
/// * `object_type` - A string slice representing the type of the object (e.g., table, view).
/// * `contents` - A string slice containing the contents of the SQL file.
/// * `stmt` - A mutable reference to a `DatabaseObject` to be updated.
///
/// # Returns
///
/// This function returns a `Result` containing:
/// * `Ok(())` - If the `DatabaseObject` was successfully updated.
/// * `Err(Box<dyn Error>)` - An error if the object name does not match the file name or if
///   there are issues parsing the SQL content.
fn relational_object_conformance(
    file_path: &Path,
    schema_name: &str,
    object_type: &str,
    stmt: &mut DatabaseObject,
) -> Result<(), Box<dyn Error>> {
    // Extract the file name from the file path
    let file_name = file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Failed to extract file stem from path: {:?}", file_path))?;

    // Parse the SQL content to extract the first SQL object
    let dialect = PostgreSqlDialect {};
    let parsed_content = match Parser::parse_sql(&dialect, &stmt.value) {
        Ok(content) => content
            .first()
            .cloned() // Clone the first element to extend its lifetime
            .ok_or("No objects found in parsed content")?,
        Err(e) => return Err(Box::new(e)),
    };

    // Use a visitor to traverse the SQL statement and gather necessary information
    let mut visitor = SqlVisitor::new();
    visitor.pre_visit_statement(&parsed_content); // Use pre_visit_statement method

    // Check if the file name matches the object name
    if file_name != visitor.object_name {
        return Err(format!(
            "Object name '{}' in file does not match name '{}' in SQL",
            file_name, visitor.object_name
        )
        .into());
    }

    // Check if the schema name matches the object schema
    if !visitor.schema_name.is_empty() && visitor.schema_name != schema_name {
        return Err(format!(
            "Schema name '{}' in file does not match schema name '{}' in SQL",
            schema_name, visitor.schema_name
        )
        .into());
    }

    // Create a unique identifier for the DatabaseObject
    let key = format!(
        "{}.{}.{}.{}",
        schema_name, object_type, file_name, stmt.change_name
    );

    // Update the existing DatabaseObject
    stmt.change_name = key;
    stmt.parsed_content = Some(parsed_content);

    Ok(())
}

/// Parses a string containing multiple SQL statements delimited by specified start and end delimiters.
///
/// This function processes the input string `content` to extract SQL statements that are enclosed
/// between the `start_delimiter` and `end_delimiter`. Each statement is associated with a set of attributes
/// defined in the `start_delimiter` line. The attributes are key-value pairs that provide additional
/// metadata for the SQL statement.
///
/// # Arguments
///
/// * `content` - A string slice that holds the entire content containing multiple SQL statements.
/// * `start_delimiter` - A string slice that marks the beginning of a SQL statement and contains attributes.
/// * `end_delimiter` - A string slice that marks the end of a SQL statement.
/// * `key` - The key used to identify the change name in the start_delimiter.
///
/// # Returns
///
/// This function returns an `IndexMap` where the keys are the unique identifiers for each SQL statement
/// (derived from the attributes or generated as "rootN" if not specified), and the values are `DatabaseObject`
/// instances containing the parsed SQL statement, its attributes, and dependencies.
fn parse_change_stmts(
    content: &str,
    start_delimiter: &str,
    end_delimiter: &str,
    key: &str,
) -> IndexMap<String, DatabaseObject> {
    let mut result: IndexMap<String, DatabaseObject> = IndexMap::new();
    let mut dependencies: HashSet<String> = HashSet::new();
    let mut value = String::new();
    let mut properties = HashMap::new();
    let mut in_statement = false;
    let mut root_counter = 0;
    let mut change_name = String::new();

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
            change_name = properties.get(key).cloned().unwrap_or_else(|| {
                let root_name = format!("root{}", root_counter);
                root_counter += 1;
                root_name
            });
        } else if line.trim() == end_delimiter {
            if in_statement {
                result.insert(
                    change_name.clone(),
                    DatabaseObject::new(
                        change_name.clone(),
                        value.trim().to_string(),
                        dependencies.clone(),
                        properties.clone(),
                        None,
                    ),
                );
                dependencies.insert(change_name.clone());
                value.clear();
                properties.clear();
                in_statement = false;
            } else {
                change_name = format!("root{}", root_counter);
                root_counter += 1;
                result.insert(
                    change_name.clone(),
                    DatabaseObject::new(
                        change_name.clone(),
                        value.trim().to_string(),
                        dependencies.clone(),
                        properties.clone(),
                        None,
                    ),
                );
                dependencies.insert(change_name.clone());
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
        change_name = format!("root{}", root_counter);
        result.insert(
            change_name.clone(),
            DatabaseObject::new(
                change_name,
                value.trim().to_string(),
                dependencies,
                properties,
                None,
            ),
        );
    }

    result
}

/// Determines the execution order of relational objects based on their dependencies.
///
/// This function takes a reference to an `IndexMap` containing `DatabaseObject` instances and their
/// associated names. Each `DatabaseObject` may have dependencies on other objects, which are represented
/// as a set of strings within the `DatabaseObject`.
///
/// The function constructs a directed graph where each node represents a `DatabaseObject` and each edge
/// represents a dependency between two objects. It then performs a topological sort on this graph to
/// determine the order in which the objects should be processed to respect their dependencies.
///
/// # Arguments
///
/// * `object_info` - A reference to an `IndexMap` where the keys are the names of the `DatabaseObject`
///   instances and the values are the corresponding `DatabaseObject` instances.
///
/// # Returns
///
/// This function returns a `Result` containing:
/// * `Ok(IndexMap<String, DatabaseObject>)` - An `IndexMap` where the keys are the names of the `DatabaseObject`
///   instances and the values are the corresponding `DatabaseObject` instances, ordered according to their
///   dependencies.
/// * `Err(Box<dyn std::error::Error>)` - An error if a cycle is detected in the dependencies, indicating that
///   it is not possible to determine a valid execution order.
///
/// # Errors
///
/// This function will return an error if a cycle is detected in the dependencies, as this would make it
/// impossible to determine a valid execution order.
fn determine_execution_order(
    object_info: &IndexMap<String, DatabaseObject>,
) -> Result<IndexMap<String, DatabaseObject>, Box<dyn std::error::Error>> {
    let mut edges = Vec::new();

    for (key, obj) in object_info {
        for dep in &obj.dependencies {
            // Find the full key for the dependency, considering both object name and change name
            if let Some(dep_key) = object_info
                .keys()
                .find(|k| k.ends_with(dep) || k.split('.').any(|part| part == dep))
            {
                edges.push((dep_key.as_str(), key.as_str()));
            } else {
                log::warn!("Dependency '{}' not found for object '{}'", dep, key);
            }
        }
    }

    let order: Vec<String> = if edges.is_empty() {
        // If there are no edges, return the keys in their original order
        object_info.keys().cloned().collect()
    } else {
        topo_sort(&edges)
            .map_err(|_| "Cycle detected in dependencies")?
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    };

    let mut ordered_object_info = IndexMap::new();
    for key in order {
        if let Some(obj) = object_info.get(&key) {
            ordered_object_info.insert(key.clone(), obj.clone());
        }
    }
    Ok(ordered_object_info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_read_source_code_with_valid_directory() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("schema1").join("table").join("table1.sql");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "CREATE TABLE table1 (id INT);").unwrap();

        let result = read_source_code(dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let object_info = result.unwrap();
        assert_eq!(object_info.len(), 1);
        assert!(object_info.contains_key("schema1.table.table1.root0"));
    }

    #[test]
    fn test_read_source_code_with_invalid_directory() {
        let result = read_source_code("/invalid/path");
        assert!(result.is_err());
    }

    #[test]
    fn test_read_source_code_with_multiple_files() {
        let dir = tempdir().unwrap();
        let file_path1 = dir.path().join("schema1").join("table").join("table1.sql");
        let file_path2 = dir.path().join("schema1").join("view").join("view1.sql");
        fs::create_dir_all(file_path1.parent().unwrap()).unwrap();
        fs::create_dir_all(file_path2.parent().unwrap()).unwrap();
        let mut file1 = File::create(&file_path1).unwrap();
        let mut file2 = File::create(&file_path2).unwrap();
        writeln!(file1, "CREATE TABLE table1 (id INT);").unwrap();
        writeln!(file2, "CREATE VIEW view1 AS SELECT * FROM table1;").unwrap();

        let result = read_source_code(dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let object_info = result.unwrap();
        assert_eq!(object_info.len(), 2);
        assert!(object_info.contains_key("schema1.table.table1.root0"));
        assert!(object_info.contains_key("schema1.view.view1.root0"));
    }

    #[test]
    fn test_read_source_code_with_dependencies() {
        let dir = tempdir().unwrap();
        let file_path1 = dir.path().join("schema1").join("table").join("table1.sql");
        let file_path2 = dir.path().join("schema1").join("table").join("table2.sql");
        let file_path3 = dir.path().join("schema1").join("table").join("table3.sql");
        let file_path4 = dir.path().join("schema1").join("table").join("table4.sql");
        fs::create_dir_all(file_path1.parent().unwrap()).unwrap();
        fs::create_dir_all(file_path2.parent().unwrap()).unwrap();
        fs::create_dir_all(file_path3.parent().unwrap()).unwrap();
        fs::create_dir_all(file_path4.parent().unwrap()).unwrap();
        let mut file1 = File::create(&file_path1).unwrap();
        let mut file2 = File::create(&file_path2).unwrap();
        let mut file3 = File::create(&file_path3).unwrap();
        let mut file4 = File::create(&file_path4).unwrap();
        writeln!(
            file1,
            "//// CHANGE name=change1\nCREATE TABLE table1 (id INT);\nGO"
        )
        .unwrap();
        writeln!(
            file2,
            "//// CHANGE name=change2 depends=table1\nCREATE TABLE table2 (id INT);\nGO"
        )
        .unwrap();
        writeln!(
            file3,
            "//// CHANGE name=change3 depends=change1\nCREATE TABLE table3 (id INT);\nGO"
        )
        .unwrap();
        writeln!(
            file4,
            "//// CHANGE name=change4 depends=table2,change3\nCREATE TABLE table4 (id INT);\nGO"
        )
        .unwrap();

        let result = read_source_code(dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let object_info = result.unwrap();
        assert_eq!(object_info.len(), 4);
        assert!(object_info.contains_key("schema1.table.table1.change1"));
        assert!(object_info.contains_key("schema1.table.table2.change2"));
        assert!(object_info.contains_key("schema1.table.table3.change3"));
        assert!(object_info.contains_key("schema1.table.table4.change4"));

        // Assert dependencies
        let change2 = object_info.get("schema1.table.table2.change2").unwrap();
        assert!(change2.dependencies.contains("table1"));

        let change3 = object_info.get("schema1.table.table3.change3").unwrap();
        assert!(change3.dependencies.contains("change1"));

        let change4 = object_info.get("schema1.table.table4.change4").unwrap();
        assert!(change4.dependencies.contains("table2"));
        assert!(change4.dependencies.contains("change3"));
    }

    #[test]
    fn test_file_name_matches_object_name() {
        let dir = tempfile::tempdir().unwrap();
        let file_path1 = dir.path().join("schema1/table/change1.sql");
        fs::create_dir_all(file_path1.parent().unwrap()).unwrap();
        let mut file1 = File::create(&file_path1).unwrap();
        writeln!(
            file1,
            "//// CHANGE name=table1\nCREATE TABLE table1 (id INT);\nGO"
        )
        .unwrap();

        let result = read_source_code(dir.path().to_str().unwrap());
        assert!(result.is_err());
        let error_message = result.unwrap_err().to_string();
        assert!(error_message
            .contains("Object name 'change1' in file does not match name 'table1' in SQL"));
    }

    #[test]
    fn test_schema_name_matches_object_schema() {
        let dir = tempfile::tempdir().unwrap();
        let file_path1 = dir.path().join("schema1/table/table1.sql");
        fs::create_dir_all(file_path1.parent().unwrap()).unwrap();
        let mut file1 = File::create(&file_path1).unwrap();
        writeln!(
            file1,
            "//// CHANGE name=table1\nCREATE TABLE schema2.table1 (id INT);\nGO"
        )
        .unwrap();

        let result = read_source_code(dir.path().to_str().unwrap());
        assert!(result.is_err());
        let error_message = result.unwrap_err().to_string();
        assert!(error_message
            .contains("Schema name 'schema1' in file does not match schema name 'schema2' in SQL"));
    }

    #[test]
    fn test_circular_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let file_path1 = dir.path().join("schema1/table/table1.sql");
        let file_path2 = dir.path().join("schema1/table/table2.sql");

        fs::create_dir_all(file_path1.parent().unwrap()).unwrap();
        fs::create_dir_all(file_path2.parent().unwrap()).unwrap();
        let mut file1 = File::create(&file_path1).unwrap();
        let mut file2 = File::create(&file_path2).unwrap();
        writeln!(
            file1,
            "//// CHANGE name=change1 depends=change2\nCREATE TABLE table1 (id INT);\nGO"
        )
        .unwrap();
        writeln!(
            file2,
            "//// CHANGE name=change2 depends=change1\nCREATE TABLE table2 (id INT);\nGO"
        )
        .unwrap();

        let result = read_source_code(dir.path().to_str().unwrap());
        assert!(result.is_err());
        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("Cycle detected in dependencies"));
    }

    #[test]
    fn test_circular_dependency_three_objects() {
        let dir = tempfile::tempdir().unwrap();
        let file_path1 = dir.path().join("schema1/table/table1.sql");
        let file_path2 = dir.path().join("schema1/table/table2.sql");
        let file_path3 = dir.path().join("schema1/table/table3.sql");

        fs::create_dir_all(file_path1.parent().unwrap()).unwrap();
        fs::create_dir_all(file_path2.parent().unwrap()).unwrap();
        fs::create_dir_all(file_path3.parent().unwrap()).unwrap();
        let mut file1 = File::create(&file_path1).unwrap();
        let mut file2 = File::create(&file_path2).unwrap();
        let mut file3 = File::create(&file_path3).unwrap();
        writeln!(
            file1,
            "//// CHANGE name=change1 depends=change3\nCREATE TABLE table1 (id INT);\nGO"
        )
        .unwrap();
        writeln!(
            file2,
            "//// CHANGE name=change2 depends=table1\nCREATE TABLE table2 (id INT);\nGO"
        )
        .unwrap();
        writeln!(
            file3,
            "//// CHANGE name=change3 depends=change2\nCREATE TABLE table3 (id INT);\nGO"
        )
        .unwrap();

        let result = read_source_code(dir.path().to_str().unwrap());
        assert!(result.is_err());
        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("Cycle detected in dependencies"));
    }

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
    fn test_parse_change_stmts_without_start_delimiters_and_one_end_statement() {
        let content =
            "CREATE FUNCTION func1() RETURNS integer\n    LANGUAGE plpgsql\n    AS '\nBEGIN\n    -- ensure that func comment remains\n    RETURN 1;\nEND;\n';\n\n\nGO";
        let parsed_stmts = parse_change_stmts(content, "//// CHANGE", "GO", "name");
        assert_eq!(parsed_stmts.len(), 1);
        assert!(parsed_stmts.contains_key("root0"));
    }

    #[test]
    fn test_parse_change_stmts_without_start_delimiters_and_multiple_statements() {
        let content = "CREATE PROCEDURE sp1() LANGUAGE plpgsql AS $$ DECLARE val INTEGER; END $$; \n\nGO\nCREATE PROCEDURE sp1(my_param INTEGER) LANGUAGE plpgsql AS $$ DECLARE val INTEGER; END $$; \n\nGO";
        let parsed_stmts = parse_change_stmts(content, "//// CHANGE", "GO", "name");
        assert_eq!(parsed_stmts.len(), 2);
        assert!(parsed_stmts.contains_key("root0"));
        assert!(parsed_stmts.contains_key("root1"));
    }

    #[test]
    fn test_read_source_code_with_one_schema() {
        let source_code = read_source_code("tests/schemas/baseline/").unwrap();
        assert_eq!(source_code.len(), 11);
        assert!(source_code.contains_key("baseline.function.func_with_overload.root0"));
    }
}
