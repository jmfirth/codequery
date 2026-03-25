//! Data models.

/// A user in the system.
#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub age: u32,
}

/// Possible user roles.
pub enum Role {
    Admin,
    Member,
    Guest,
}

/// Alias for user ID.
pub type UserId = u64;

pub(crate) const DEFAULT_NAME: &str = "anonymous";
