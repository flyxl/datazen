use crate::DefaultTableSort;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultOrderOverride {
    Inherit,
    Order(DefaultTableSort),
}

pub fn resolve_default_table_sort(
    override_order: DefaultOrderOverride,
    setting: DefaultTableSort,
) -> DefaultTableSort {
    match override_order {
        DefaultOrderOverride::Inherit => setting,
        DefaultOrderOverride::Order(order) => order,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherit_returns_the_global_setting_unchanged() {
        for setting in [
            DefaultTableSort::PrimaryKeyAsc,
            DefaultTableSort::PrimaryKeyDesc,
            DefaultTableSort::None,
        ] {
            assert_eq!(resolve_default_table_sort(DefaultOrderOverride::Inherit, setting), setting);
        }
    }

    #[test]
    fn an_explicit_override_wins_over_the_global_setting() {
        for override_order in [
            DefaultTableSort::PrimaryKeyAsc,
            DefaultTableSort::PrimaryKeyDesc,
            DefaultTableSort::None,
        ] {
            for setting in [
                DefaultTableSort::PrimaryKeyAsc,
                DefaultTableSort::PrimaryKeyDesc,
                DefaultTableSort::None,
            ] {
                assert_eq!(
                    resolve_default_table_sort(DefaultOrderOverride::Order(override_order), setting),
                    override_order
                );
            }
        }
    }

    #[test]
    fn the_override_order_is_resolved_before_the_setting() {
        assert_eq!(
            resolve_default_table_sort(
                DefaultOrderOverride::Order(DefaultTableSort::None),
                DefaultTableSort::PrimaryKeyDesc
            ),
            DefaultTableSort::None
        );
    }
}
