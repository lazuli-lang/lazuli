use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    pub name: String,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    pub fields: Vec<Field>,
    pub commands: Vec<Command>,
    pub queries: Vec<Query>,
    pub surfaces: Vec<Surface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub kind: String,
    pub required: bool,
    pub unique: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub input: Vec<String>,
    pub policy: Option<String>,
    pub emits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub name: String,
    pub search: Vec<String>,
    pub filters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    pub name: String,
    pub list_columns: Vec<String>,
    pub form_fields: Vec<String>,
    pub detail_fields: Vec<String>,
}
