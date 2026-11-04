//! Native TDS parameter conversion for the driver-api value model.

use datazen_driver_api::Value;
use tiberius::ToSql;

/// Owned values are kept beside the `ToSql` references for the duration of a
/// Tiberius request. SQL text never receives a serialized parameter value.
pub(crate) enum BoundParameter {
    Null(Option<String>),
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
}

impl BoundParameter {
    pub(crate) fn as_tosql(&self) -> &dyn ToSql {
        match self {
            Self::Null(value) => value,
            Self::Bool(value) => value,
            Self::Integer(value) => value,
            Self::Float(value) => value,
            Self::String(value) => value,
            Self::Bytes(value) => value,
        }
    }
}

pub(crate) fn bind_values(values: &[Value]) -> Vec<BoundParameter> {
    values
        .iter()
        .map(|value| match value {
            Value::Null => BoundParameter::Null(None),
            Value::Bool(value) => BoundParameter::Bool(*value),
            Value::Integer(value) => BoundParameter::Integer(*value),
            Value::Float(value) => BoundParameter::Float(*value),
            Value::String(value) | Value::Timestamp(value) => BoundParameter::String(value.clone()),
            Value::Bytes(value) => BoundParameter::Bytes(value.clone()),
            Value::Json(value) => BoundParameter::String(value.to_string()),
        })
        .collect()
}

pub(crate) fn to_sql_refs(values: &[BoundParameter]) -> Vec<&dyn ToSql> {
    values.iter().map(BoundParameter::as_tosql).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiberius::ColumnData;

    #[test]
    fn values_convert_to_native_tds_parameter_families() {
        let input = vec![
            Value::Null,
            Value::Bool(true),
            Value::Integer(7),
            Value::Float(2.5),
            Value::String("O'Brien".into()),
            Value::Bytes(vec![0, 255]),
            Value::Timestamp("2026-01-02T03:04:05Z".into()),
            Value::Json(serde_json::json!({ "x": 1 })),
        ];
        let bound = bind_values(&input);
        let refs = to_sql_refs(&bound);
        let tds = refs.iter().map(|value| value.to_sql()).collect::<Vec<_>>();

        assert!(matches!(tds[0], ColumnData::String(None)));
        assert!(matches!(tds[1], ColumnData::Bit(Some(true))));
        assert!(matches!(tds[2], ColumnData::I64(Some(7))));
        assert!(matches!(tds[3], ColumnData::F64(Some(value)) if value == 2.5));
        assert!(matches!(tds[4], ColumnData::String(Some(_))));
        assert!(matches!(tds[5], ColumnData::Binary(Some(_))));
        assert!(matches!(tds[6], ColumnData::String(Some(_))));
        assert!(matches!(tds[7], ColumnData::String(Some(_))));
    }
}
