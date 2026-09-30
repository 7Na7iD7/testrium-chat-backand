use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Student,
    Professor,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Student => "student",
            Role::Professor => "professor",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub auth_user_id: String,
    pub role: Role,
    pub identifier: String,
    pub display_name: String,
    pub accessible_section_ids: Vec<String>,
}
