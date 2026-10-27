#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
    use datazen::services::schema_scope::*;
    use datazen_driver_api::DatabaseDriver;

    fn schema_aware() -> Arc<dyn DatabaseDriver> {
        MockDriver::new(
            "postgres",
            MockDriverOptions {
                has_schema_level: true,
                default_schema: Some("public"),
                ..Default::default()
            },
        )
    }

    fn schema_less() -> Arc<dyn DatabaseDriver> {
        MockDriver::new("postgres", MockDriverOptions::default())
    }

    #[test]
    fn explicit_wins_over_everything() {
        let driver = schema_aware();
        assert_eq!(
            metadata_schema(
                driver.as_ref(),
                Some("chosen"),
                Some("from_table"),
                Some("cfg")
            )
            .as_deref(),
            Some("chosen")
        );
    }

    #[test]
    fn falls_back_in_order_then_to_the_driver_default() {
        let driver = schema_aware();
        assert_eq!(
            metadata_schema(driver.as_ref(), None, Some("from_table"), Some("cfg")).as_deref(),
            Some("from_table")
        );
        assert_eq!(
            metadata_schema(driver.as_ref(), None, None, Some("cfg")).as_deref(),
            Some("cfg")
        );
        assert_eq!(
            metadata_schema(driver.as_ref(), None, None, None).as_deref(),
            Some("public")
        );
    }

    #[test]
    fn blank_values_are_absent_not_a_schema() {
        let driver = schema_aware();
        assert_eq!(
            metadata_schema(driver.as_ref(), Some("   "), Some(""), Some("real")).as_deref(),
            Some("real")
        );
    }

    #[test]
    fn schema_less_driver_always_gets_none() {
        let driver = schema_less();
        // Even an explicit schema is dropped: passing it on would be rejected
        // by the driver's own validator, and silently keeping it would be the
        // implicit-schema bug this contract exists to remove.
        assert_eq!(
            metadata_schema(
                driver.as_ref(),
                Some("public"),
                Some("public"),
                Some("public")
            ),
            None
        );
    }
}
