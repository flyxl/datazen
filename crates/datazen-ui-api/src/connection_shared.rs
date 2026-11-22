#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub database: Option<String>,
}
