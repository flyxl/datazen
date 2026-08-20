use datazen_driver_api::*;
use serde_json::json;

pub fn sqlserver_admin_command_definitions() -> Vec<DriverCommandDefinition> {
    let mut cmds = vec![
        query_command_definition(),
        execute_command_definition(),
        query_stream_command_definition(),
    ];

    cmds.push(DriverCommandDefinition {
        id: "create_database".into(),
        name: "Create Database".into(),
        description: Some("Create a new SQL Server database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Database name" }
            },
            "required": ["name"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "create_schema".into(),
        name: "Create Schema".into(),
        description: Some("Create a new schema in the current database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Schema name" },
                "owner": { "type": "string", "description": "Schema owner" }
            },
            "required": ["name"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "create_user".into(),
        name: "Create User".into(),
        description: Some("Create a new SQL Server login and user".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "username": { "type": "string", "description": "Username" },
                "password": { "type": "string", "description": "Password" }
            },
            "required": ["username"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "drop_database".into(),
        name: "Drop Database".into(),
        description: Some("Drop a SQL Server database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Database name to drop" }
            },
            "required": ["name"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds
}

pub fn build_admin_sql(command: &str, input: &serde_json::Value) -> Result<String, DriverError> {
    match command {
        "drop_database" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            Ok(format!("DROP DATABASE [{}]", name.replace(']', "]]")))
        }
        "create_database" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            Ok(format!("CREATE DATABASE [{}]", name.replace(']', "]]")))
        }
        "create_schema" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            let mut sql = format!("CREATE SCHEMA [{}]", name.replace(']', "]]"));
            if let Some(owner) = input["owner"].as_str() {
                sql.push_str(&format!(" AUTHORIZATION [{}]", owner.replace(']', "]]")));
            }
            Ok(sql)
        }
        "create_user" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let escaped_name = username.replace('\'', "''");
            let bracket_name = username.replace(']', "]]");
            let mut sql = format!(
                "CREATE LOGIN [{}] WITH PASSWORD = '{}'",
                bracket_name,
                input["password"].as_str().unwrap_or("").replace('\'', "''")
            );
            sql.push_str(&format!(
                "; CREATE USER [{}] FOR LOGIN [{}]",
                bracket_name, bracket_name
            ));
            let _ = escaped_name;
            Ok(sql)
        }
        _ => Err(DriverError::Unsupported(format!(
            "unsupported admin command: {command}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definitions_include_admin_commands() {
        let defs = sqlserver_admin_command_definitions();
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"query"));
        assert!(ids.contains(&"execute"));
        assert!(ids.contains(&"create_database"));
        assert!(ids.contains(&"create_schema"));
        assert!(ids.contains(&"create_user"));
    }

    #[test]
    fn create_database_basic() {
        let input = json!({ "name": "testdb" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, "CREATE DATABASE [testdb]");
    }

    #[test]
    fn create_database_escapes_brackets() {
        let input = json!({ "name": "test]db" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, "CREATE DATABASE [test]]db]");
    }

    #[test]
    fn create_schema_basic() {
        let input = json!({ "name": "analytics" });
        let sql = build_admin_sql("create_schema", &input).unwrap();
        assert_eq!(sql, "CREATE SCHEMA [analytics]");
    }

    #[test]
    fn create_schema_with_owner() {
        let input = json!({ "name": "hr", "owner": "hr_admin" });
        let sql = build_admin_sql("create_schema", &input).unwrap();
        assert_eq!(sql, "CREATE SCHEMA [hr] AUTHORIZATION [hr_admin]");
    }

    #[test]
    fn create_user_basic() {
        let input = json!({ "username": "reader", "password": "pass123" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert!(sql.contains("CREATE LOGIN [reader]"));
        assert!(sql.contains("PASSWORD = 'pass123'"));
        assert!(sql.contains("CREATE USER [reader] FOR LOGIN [reader]"));
    }

    #[test]
    fn missing_name_returns_error() {
        let input = json!({});
        assert!(build_admin_sql("create_database", &input).is_err());
        assert!(build_admin_sql("create_schema", &input).is_err());
    }

    #[test]
    fn missing_username_returns_error() {
        let input = json!({});
        assert!(build_admin_sql("create_user", &input).is_err());
    }

    #[test]
    fn drop_database_basic() {
        let input = json!({ "name": "testdb" });
        let sql = build_admin_sql("drop_database", &input).unwrap();
        assert_eq!(sql, "DROP DATABASE [testdb]");
    }

    #[test]
    fn unknown_command_returns_error() {
        let input = json!({ "name": "x" });
        assert!(build_admin_sql("some_unknown_cmd", &input).is_err());
    }
}
