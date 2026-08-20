use datazen_driver_api::*;
use serde_json::json;

pub fn mysql_admin_command_definitions() -> Vec<DriverCommandDefinition> {
    let mut cmds = vec![query_command_definition(), execute_command_definition()];

    cmds.push(DriverCommandDefinition {
        id: "create_database".into(),
        name: "Create Database".into(),
        description: Some("Create a new MySQL database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Database name" },
                "encoding": { "type": "string", "description": "Character set (e.g. utf8mb4)" }
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
        description: Some("Create a new MySQL user".into()),
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
        id: "grant_privileges".into(),
        name: "Grant Privileges".into(),
        description: Some("Grant privileges to a user on a database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "username": { "type": "string", "description": "Target user" },
                "database": { "type": "string", "description": "Target database (empty for all)" },
                "privileges": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Privileges (e.g. SELECT, INSERT, UPDATE, DELETE, CREATE, ALL PRIVILEGES)"
                },
                "grantOption": { "type": "boolean", "description": "WITH GRANT OPTION" }
            },
            "required": ["username", "privileges"],
            "x-datazen": {
                "privilegeGroups": [
                    {
                        "label": "Privileges",
                        "privileges": [
                            "SELECT",
                            "INSERT",
                            "UPDATE",
                            "DELETE",
                            "CREATE",
                            "DROP",
                            "ALTER",
                            "INDEX"
                        ]
                    }
                ]
            }
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "drop_user".into(),
        name: "Drop User".into(),
        description: Some("Drop a MySQL user".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "username": { "type": "string", "description": "Username to drop" }
            },
            "required": ["username"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "revoke_privileges".into(),
        name: "Revoke Privileges".into(),
        description: Some("Revoke privileges from a user".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "username": { "type": "string", "description": "Target user" },
                "database": { "type": "string", "description": "Target database (empty for all)" },
                "privileges": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Privileges to revoke"
                }
            },
            "required": ["username", "privileges"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "drop_database".into(),
        name: "Drop Database".into(),
        description: Some("Drop a MySQL database".into()),
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

    cmds.extend(schema_object_command_definitions());

    cmds
}

pub fn build_admin_sql(command: &str, input: &serde_json::Value) -> Result<String, DriverError> {
    match command {
        "drop_database" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            Ok(format!("DROP DATABASE `{}`", name.replace('`', "``")))
        }
        "create_database" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            let mut sql = format!("CREATE DATABASE `{}`", name.replace('`', "``"));
            if let Some(encoding) = input["encoding"].as_str() {
                let safe = encoding
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>();
                if !safe.is_empty() {
                    sql.push_str(&format!(" CHARACTER SET {}", safe));
                }
            }
            Ok(sql)
        }
        "create_user" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let password = input["password"].as_str().unwrap_or("");
            Ok(format!(
                "CREATE USER '{}'@'%' IDENTIFIED BY '{}'",
                username.replace('\'', "''"),
                password.replace('\'', "''")
            ))
        }
        "drop_user" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            Ok(format!("DROP USER '{}'@'%'", username.replace('\'', "''")))
        }
        "revoke_privileges" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let privileges = input["privileges"]
                .as_array()
                .ok_or_else(|| DriverError::InvalidConfig("privileges array is required".into()))?
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>();
            if privileges.is_empty() {
                return Err(DriverError::InvalidConfig(
                    "at least one privilege is required".into(),
                ));
            }
            let priv_str = privileges.join(", ");
            let database = input["database"].as_str().unwrap_or("");
            let target = if database.is_empty() {
                "*.*".to_string()
            } else {
                format!("`{}`.* ", database.replace('`', "``"))
            };
            Ok(format!(
                "REVOKE {} ON {} FROM '{}'@'%'",
                priv_str,
                target,
                username.replace('\'', "''")
            ))
        }
        "grant_privileges" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let privileges = input["privileges"]
                .as_array()
                .ok_or_else(|| DriverError::InvalidConfig("privileges array is required".into()))?
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>();
            if privileges.is_empty() {
                return Err(DriverError::InvalidConfig(
                    "at least one privilege is required".into(),
                ));
            }
            let priv_str = privileges.join(", ");
            let database = input["database"].as_str().unwrap_or("");
            let target = if database.is_empty() {
                "*.*".to_string()
            } else {
                format!("`{}`.* ", database.replace('`', "``"))
            };
            let mut sql = format!(
                "GRANT {} ON {} TO '{}'@'%'",
                priv_str,
                target,
                username.replace('\'', "''")
            );
            if input["grantOption"].as_bool().unwrap_or(false) {
                sql.push_str(" WITH GRANT OPTION");
            }
            Ok(sql)
        }
        _ => Err(DriverError::Unsupported(format!(
            "unsupported admin command: {command}"
        ))),
    }
}

pub async fn execute_mysql_admin_command(
    pool: &sqlx::MySqlPool,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    let sql = build_admin_sql(command, &input)?;
    sqlx::query(&sql)
        .execute(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    Ok(CommandResult {
        data: json!({ "ok": true }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definitions_include_admin_commands() {
        let defs = mysql_admin_command_definitions();
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"query"));
        assert!(ids.contains(&"execute"));
        assert!(ids.contains(&"create_database"));
        assert!(ids.contains(&"create_user"));
        assert!(ids.contains(&"list_objects"));
        assert!(ids.contains(&"get_object_ddl"));
        assert!(ids.contains(&"list_privileges"));
    }

    #[test]
    fn create_database_basic() {
        let input = json!({ "name": "testdb" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, "CREATE DATABASE `testdb`");
    }

    #[test]
    fn create_database_with_encoding() {
        let input = json!({ "name": "mydb", "encoding": "utf8mb4" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, "CREATE DATABASE `mydb` CHARACTER SET utf8mb4");
    }

    #[test]
    fn create_database_escapes_backticks() {
        let input = json!({ "name": "test`db" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, "CREATE DATABASE `test``db`");
    }

    #[test]
    fn create_user_basic() {
        let input = json!({ "username": "reader" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, "CREATE USER 'reader'@'%' IDENTIFIED BY ''");
    }

    #[test]
    fn create_user_with_password() {
        let input = json!({ "username": "admin", "password": "s3cret" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, "CREATE USER 'admin'@'%' IDENTIFIED BY 's3cret'");
    }

    #[test]
    fn create_user_escapes_quotes() {
        let input = json!({ "username": "o'brien", "password": "it's" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, "CREATE USER 'o''brien'@'%' IDENTIFIED BY 'it''s'");
    }

    #[test]
    fn missing_name_returns_error() {
        let input = json!({});
        assert!(build_admin_sql("create_database", &input).is_err());
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
        assert_eq!(sql, "DROP DATABASE `testdb`");
    }

    #[test]
    fn unknown_command_returns_error() {
        let input = json!({ "name": "x" });
        assert!(build_admin_sql("some_unknown_cmd", &input).is_err());
    }
}
