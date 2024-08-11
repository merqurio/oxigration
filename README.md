# Oxigration: SQL Migration Manager

## Project Overview

Oxigration is a SQL migration manager designed to implement database schemas based on individual files defining SQL objects organized in directories. This tool simplifies the process of managing and applying database schema changes across different environments.

## Features

- Parses SQL object definitions from separate files
- Organizes SQL objects in a directory structure
- Generates and applies migration scripts
- Tracks schema version and applied migrations
- Supports multiple database environments

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

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

Please make sure to update tests as appropriate and adhere to the existing coding style.

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details.

## Contact

Your Name - [@your_twitter](https://twitter.com/your_twitter) - email@example.com

Project Link: [https://github.com/yourusername/oxigration](https://github.com/yourusername/oxigration)