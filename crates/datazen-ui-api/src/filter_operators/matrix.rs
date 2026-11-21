use crate::CellType::{Bool, Bytes, Date, DateTime, Enum, Json, Number, Text, Time, Unknown, Uuid};
use crate::FilterOperator::{
    Between, Contains, EndsWith, Eq, Gt, Gte, In, IsEmpty, IsNotEmpty, IsNotNull, IsNull,
    JsonContains, Like, Lt, Lte, Ne, NotBetween, NotContains, NotIn, NotRegex, Regex, StartsWith,
};
use crate::{CellType, FilterOperator};

pub const ALL_CELL_TYPES: &[CellType] = &[
    Text, Enum, Number, Bool, Date, Time, DateTime, Json, Bytes, Uuid, Unknown,
];

pub fn operators_by_cell_type(cell_type: CellType) -> &'static [FilterOperator] {
    match cell_type {
        Text | Unknown => &[
            Eq,
            Ne,
            Gt,
            Lt,
            Gte,
            Lte,
            Like,
            Contains,
            NotContains,
            StartsWith,
            EndsWith,
            In,
            NotIn,
            IsNull,
            IsNotNull,
            IsEmpty,
            IsNotEmpty,
        ],
        Enum | Uuid => &[Eq, Ne, In, NotIn, IsNull, IsNotNull],
        Number => &[
            Eq, Ne, Gt, Lt, Gte, Lte, Between, NotBetween, In, NotIn, IsNull, IsNotNull,
        ],
        Bool => &[Eq, Ne, IsNull, IsNotNull],
        Date | Time | DateTime => &[
            Eq, Ne, Gt, Lt, Gte, Lte, Between, NotBetween, IsNull, IsNotNull,
        ],
        Json => &[Eq, Ne, IsNull, IsNotNull],
        Bytes => &[Eq, Ne, IsNull, IsNotNull],
    }
}

pub fn dialect_operators_by_cell_type(cell_type: CellType) -> &'static [FilterOperator] {
    match cell_type {
        Text | Unknown => &[Regex, NotRegex],
        Json => &[JsonContains],
        Number | Enum | Bool | Date | Time | DateTime | Bytes | Uuid => &[],
    }
}

pub fn cell_types_for_operator(operator: FilterOperator) -> Vec<CellType> {
    ALL_CELL_TYPES
        .iter()
        .copied()
        .filter(|cell_type| {
            operators_by_cell_type(*cell_type).contains(&operator)
                || dialect_operators_by_cell_type(*cell_type).contains(&operator)
        })
        .collect()
}
