use std::collections::HashMap;

use super::types::CellContext;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DomElement {
    pub attributes: HashMap<String, String>,
    pub parent: Option<Box<DomElement>>,
}

impl DomElement {
    pub fn leaf(attributes: Vec<(&str, &str)>) -> Self {
        Self {
            attributes: attributes
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
            parent: None,
        }
    }

    pub fn child_of(attributes: Vec<(&str, &str)>, parent: DomElement) -> Self {
        Self {
            attributes: attributes
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
            parent: Some(Box::new(parent)),
        }
    }
}

pub fn resolve_data_table_cell_from_event(target: Option<&DomElement>) -> Option<CellContext> {
    let mut current = target;
    while let Some(element) = current {
        if let (Some(row), Some(column)) = (
            element.attributes.get("data-dt-row"),
            element.attributes.get("data-dt-col"),
        ) {
            if column.is_empty() {
                return None;
            }
            let row_index = row.parse::<i64>().ok()?;
            if row_index < 0 {
                return None;
            }
            return Some(CellContext {
                row_index,
                column_name: Some(column.clone()),
            });
        }
        current = element.parent.as_deref();
    }
    None
}

pub fn resolve_data_table_header_col_from_event(target: Option<&DomElement>) -> Option<String> {
    let mut current = target;
    while let Some(element) = current {
        if let Some(column) = element.attributes.get("data-col-header") {
            return Some(column.clone());
        }
        current = element.parent.as_deref();
    }
    None
}
