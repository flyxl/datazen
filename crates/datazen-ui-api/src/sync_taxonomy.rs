pub fn normalize_driver_id(raw: &str) -> String {
    let lower = raw.to_lowercase();
    match lower.as_str() {
        "postgres" => "postgresql".to_owned(),
        "mssql" => "sqlserver".to_owned(),
        "presto" => "trino".to_owned(),
        "tidb" | "oceanbase" => "mysql".to_owned(),
        _ => lower,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_aliases_map_to_canonical_ids() {
        assert_eq!(normalize_driver_id("postgres"), "postgresql");
        assert_eq!(normalize_driver_id("mssql"), "sqlserver");
        assert_eq!(normalize_driver_id("presto"), "trino");
        assert_eq!(normalize_driver_id("tidb"), "mysql");
        assert_eq!(normalize_driver_id("oceanbase"), "mysql");
    }

    #[test]
    fn aliases_are_case_insensitive() {
        assert_eq!(normalize_driver_id("POSTGRES"), "postgresql");
        assert_eq!(normalize_driver_id("MsSql"), "sqlserver");
        assert_eq!(normalize_driver_id("Presto"), "trino");
        assert_eq!(normalize_driver_id("TiDB"), "mysql");
        assert_eq!(normalize_driver_id("OceanBase"), "mysql");
    }

    #[test]
    fn unknown_ids_are_lowercased_pass_through() {
        assert_eq!(normalize_driver_id("PostgreSQL"), "postgresql");
        assert_eq!(normalize_driver_id("MySQL"), "mysql");
        assert_eq!(normalize_driver_id("SQLITE"), "sqlite");
        assert_eq!(normalize_driver_id("  Redis  "), "  redis  ");
        assert_eq!(normalize_driver_id(""), "");
    }

    #[test]
    fn canonical_ids_are_stable_fixed_points() {
        for id in ["postgresql", "sqlserver", "trino", "mysql", "sqlite", "redis"] {
            assert_eq!(normalize_driver_id(id), id);
        }
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::normalize_driver_id;
    }
}
