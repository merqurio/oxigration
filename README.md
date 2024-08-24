# Oxigration: DBMS Schema Migration Manager

Oxigration is a tool for managing database schema migrations, where a DBMS (Database Management System) organizes and maintains the structure of data, and a schema defines the blueprint of how data is stored and accessed. This tool simplifies the process of managing and applying database schema changes across different environments.

**Features**

- Migrate the DBMS by applying SQL changes accurately
  - Takes DBMS objects represented in files and migrates the DBMS
  - Tracks schema version and applied migrations
- Generate DBMS objects source code from the DBMS
- Supports multiple database environments and conditional logic
- Schemas are directories, with each DBMS object type in its own subdirectory

**Example Directory Layout**

```sh
my_schema/
├── function
│   ├── func1.sql
│   └── func_with_overload.sql
├── sequence
│   ├── regular_sequence.sql
├── sp
│   └── sp1.sql
├── table
│   ├── table_a_multicol_pk.sql
│   ├── table_b_with_fk.sql
├── usertype
│   └── usertype1.sql
└── view
    └── view1.sql
```

## Overview

Oxigration is designed to make deploying changes to your environment safe and repeatable, addressing several key challenges:

1. There is no built-in way to deploy all your code at once for upgrades.
2. Some database objects need step-by-step commands to change them.
3. Changes to objects need to be applied in a specific order, which you have to set manually.

Databases face unique challenges when managing changes, especially with SQL Data Definition Language (DDL) changes to schemas.

### Key Points

- **Schema Definition and Updates:** Currently, there is no built-in mechanism to define an entire database schema in source code and apply it seamlessly to both new and existing databases. Developers must manually handle schema definitions and updates.
- **Incremental Table Modifications:** Modifying tables requires incremental statements. Unlike stateless objects such as stored procedures and views, which can be redefined without considering their previous state, tables need careful step-by-step changes to ensure data integrity and consistency.
- **Order of Changes:** The order in which changes are deployed is crucial. Dependencies between objects must be respected, such as creating tables before creating views that depend on those tables. Failing to do so can result in errors and failed deployments.

Oxigration addresses these issues by managing both stateful and stateless objects. It acts as a "mini-compiler" that processes code files and applies changes incrementally, ensuring that all dependencies and order of operations are correctly handled.

### Comparison with Other Migration Tools

Traditional migration tools often rely on "up" and "down" scripts or patches to manage database changes. These tools require developers to write separate scripts for applying and reverting changes, which can be cumbersome and error-prone. Additionally, managing the order of these scripts and ensuring that dependencies are respected can be challenging.

Oxigration simplifies this process by automatically determining the correct order of changes and managing dependencies between objects. It eliminates the need for separate "up" and "down" scripts or patches, reducing the risk of errors and making the deployment process more efficient and reliable. Unlike traditional tools, Oxigration provides a seamless mechanism to define an entire database schema in source code and apply it to both new and existing databases without manual intervention. This ensures that all changes are applied in the correct order, respecting dependencies and maintaining data integrity and consistency.

## Problem Statement and Terminology

Managing database schema changes is a complex and error-prone process. Developers need a reliable and automated way to apply changes to both new and existing databases, ensuring data integrity and consistency. The lack of built-in mechanisms to handle schema definitions, incremental modifications, and the correct order of changes poses significant challenges.

### Problem Terminology

To better understand the challenges and solutions, let's define some key terms:

| Role                 | Description                            | Database Equivalent                    |
|----------------------|----------------------------------------|----------------------------------------|
| Developers           | Individuals who create and build       | Developers and Database Administrators |
| Source Code          | Code written by developers             | DDLs, SQL scripts                      |
| Binary / Interpreter | Compiled or code ready for deployment  | Oxigration                             |
| Environment          | Running instance of the system         | DBMS(s) with desired schemas           |
| Deploy Tool          | Tool that deploys the compiled code    | CI/CD executing Oxigration on the DBMS |


## Getting Started

### Prerequisites

- Rust programming language (latest stable version)
- Cargo package manager
- Database system (PostgreSQL, MySQL, SQLite, or MSSQL)

### Installation

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/oxigration.git
   cd oxigration
   ```

2. Build the project:
   ```
   cargo build --release
   ```

3. The binary will be available in `target/release/migrate`.

### Usage

To run Oxigration:

```
cargo run --bin migrate
```

By default, it uses the following settings:
- Base directory: `tests/schemas`
- Connection string: `sqlite:///memory`

To customize these settings, modify the `src/bin/migrate.rs` file.

## Directory Structure

Oxigration expects the following directory structure for organizing SQL object files:

```
base_dir/
├── db_schema_name_1/
│   ├── object_type_1/
│   │   ├── object_1_1.sql
│   │   ├── object_1_2.sql
│   │   └── ...
│   └── object_type_2/
│       ├── object_2_1.sql
│       ├── object_2_2.sql
│       └── ...
├── db_schema_name_2/
│   ├── ...
└── ...
```

Each SQL file should contain change statements delimited by:
- Start delimiter: `//// CHANGE name=`
- End delimiter: `GO`
jG
## Deployment Algorithm

The deployment algorithm in Oxigration ensures that changes to the database are applied safely and in the correct order. This process is designed to be idempotent, meaning that applying the same changes multiple times will not have adverse effects. Here's a detailed explanation of each step:

### Steps

1. **Read Changes in Source Code**
   - Oxigration starts by reading the changes defined in the source code. These changes are typically SQL DDL scripts or other database modification commands that developers have written and stored in the version control system (like git).

2. **Read Changes from Deploy Log**
   - The deploy log is a record of all changes that have been previously applied to the environment. Oxigration reads this log to understand the current state of the environment and to determine which changes have already been applied.

3. **Calculate ChangeSet between Source Code and Deploy Log**
   - Oxigration compares the changes in the source code with the entries in the deploy log. This comparison helps identify the differences, known as the ChangeSet. The ChangeSet includes new changes that need to be applied, changes that have been modified, and any changes that have been removed.

4. **Apply ChangeSet to Environment and Deploy Log**
   - Finally, Oxigration applies the identified ChangeSet to the environment. This step involves executing the necessary SQL commands or other database modifications. After successfully applying the changes, Oxigration updates the deploy log to reflect the new state of the environment.

### Tracking Changes in the Database Management System (DBMS)

To ensure that changes are applied correctly and to track the state of each change, Oxigration uses a hashing mechanism. Each change in the source code is hashed, and this hash is stored in the deploy log. This allows Oxigration to compare the current state of the source code with the deploy log and determine what actions need to be taken.

- **Stateful vs. Stateless Changes**
  - Stateful changes to stateful DBMS objects, such as table modifications, must be applied incrementally and only once to ensure data integrity. These changes are tracked using hashes to detect any modifications or deletions.
  - Stateless changes to stateless DBMS objects, such as stored procedures and views, can be modified or deleted without concern for their previous state. These changes are also tracked using hashes, but the behavior on hash differences allows for re-deployment or removal as needed.

- **Order of Changes**
  - The order in which changes are applied is crucial. Dependencies between objects must be respected, such as creating tables before creating views that depend on those tables. Oxigration uses topological sorting to determine the correct order of changes based on dependencies.

- **File Organization**
  - Oxigration organizes changes by storing each database object in its own file. This approach is similar to how code libraries are structured, where each function or module is defined in a separate file. By isolating each database object (such as tables, views, or stored procedures) in its own file, it becomes easier to manage, review, and track changes specific to that object. This organization helps developers quickly locate and modify the relevant SQL code for a particular database object without sifting through large, monolithic scripts.

#### Stateful Changes to Stateful DBMS Objects

Stateful changes to stateful DBMS objects must only be run once. Modifying or deleting already deployed changes is not allowed. A hash of the Change text is stored in the Deploy Log for validation.

Example of stateful changes:
- Adding a new column to an existing table
- Modifying the data type of a column in a table

| Hash Comparison                                   | Action                                           |
|---------------------------------------------------|--------------------------------------------------|
| Hashes match in Source Code and Deploy Log        | No action                                        |
| Hashes in Source Code, but not Deploy Log         | Deploy Change                                    |
| Hashes in Deploy Log, but not Source Code         | Exception - Source Code Change was removed       |
| Hashes differ between Source Code and Deploy Log  | Exception - Source Code Change was modified      |

#### Stateless Changes to Stateless DBMS Objects

Stateless changes to stateless DBMS objects can be modified or deleted. The hash calculation logic remains the same, but the behavior on hash differences changes.

Example of stateless changes:
- Creating or modifying a stored procedure
- Creating or modifying a view

| Hash Comparison                                   | Action                                           |
|---------------------------------------------------|--------------------------------------------------|
| Hashes match in Source Code and Deploy Log        | No action                                        |
| Hashes in Source Code, but not Deploy Log         | Deploy Change                                    |
| Hashes in Deploy Log, but not Source Code         | Remove Change                                    |
| Hashes differ between Source Code and Deploy Log  | Re-deploy Change (drop/add if necessary)         |


### Sorting Changes

Changes are sorted using topological sort based on dependencies. Dependencies are discovered by searching for object names in the code or defined via metadata attributes.

### Integrating DB Deployments and Other Platforms

Oxigration's algorithms are platform-agnostic, with specific implementations for applying changes, reading source code, and maintaining deploy logs.

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details.