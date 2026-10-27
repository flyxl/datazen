//! Exact identities used to prove cross-kind schema migration dependencies.

use datazen_driver_api::ObjectKind;

/// Structured object identity. Display labels are for diagnostics only and
/// are never used as proof of dependency identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaObjectIdentity {
    pub kind: ObjectKind,
    pub schema: Option<String>,
    pub name: String,
    pub signature: Option<String>,
    pub target_schema: Option<String>,
    pub target_name: Option<String>,
}

/// Backend-read dependency state retained with a one-shot reviewed plan.
/// `None` means the driver could not prove a complete dependency set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaObjectDependencySnapshot {
    pub identity: SchemaObjectIdentity,
    pub dependencies: Option<Vec<SchemaObjectIdentity>>,
    /// Optional per-use evidence returned by drivers that can distinguish a
    /// declared column type from expression and constraint dependencies.
    /// `None` means the driver did not provide attribution; an empty list is
    /// a complete proof that no type dependency uses were reported.
    pub type_dependency_usages: Option<Vec<TypeDependencyUsage>>,
    /// Exact column defaults that use a sequence. `None` means the driver did
    /// not provide enough attribution to safely split an owned sequence from
    /// its owner-table creation.
    pub sequence_dependency_usages: Option<Vec<SequenceDependencyUsage>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SequenceDependencyUsage {
    pub sequence: SchemaObjectIdentity,
    pub owner_table: SchemaObjectIdentity,
    pub column_name: String,
    pub usage: SequenceDependencyUsageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SequenceDependencyUsageKind {
    ColumnDefault,
    OwnedBy,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SequenceOwnershipIdentity {
    pub schema: String,
    pub table: String,
    pub column: String,
}

impl SequenceOwnershipIdentity {
    pub fn owner_table(&self) -> SchemaObjectIdentity {
        SchemaObjectIdentity::table(Some(&self.schema), &self.table)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeDependencyUsage {
    pub dependency: SchemaObjectIdentity,
    pub usage: TypeDependencyUsageKind,
    pub column_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypeDependencyUsageKind {
    ColumnType,
    Expression,
    Constraint,
}

impl SchemaObjectIdentity {
    pub fn table(schema: Option<&str>, name: &str) -> Self {
        Self {
            kind: ObjectKind::Table,
            schema: schema.map(str::to_owned),
            name: name.to_owned(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    pub fn display_key(&self) -> String {
        let qualified = self
            .schema
            .as_deref()
            .filter(|schema| !schema.is_empty())
            .map(|schema| format!("{schema}.{}", self.name))
            .unwrap_or_else(|| self.name.clone());
        match self.kind {
            ObjectKind::Function | ObjectKind::Procedure => format!(
                "{}:{qualified}({})",
                self.kind.as_str(),
                self.signature.as_deref().unwrap_or_default()
            ),
            ObjectKind::Trigger => format!(
                "trigger:{qualified}@{}.{}",
                self.target_schema.as_deref().unwrap_or_default(),
                self.target_name.as_deref().unwrap_or_default()
            ),
            _ => format!("{}:{qualified}", self.kind.as_str()),
        }
    }
}
