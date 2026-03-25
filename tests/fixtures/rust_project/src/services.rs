//! Service implementations.

use crate::models::User;
use crate::traits::{Validate, Summary};

impl User {
    /// Create a new user.
    pub fn new(name: &str, age: u32) -> Self {
        Self {
            name: name.to_string(),
            age,
        }
    }

    /// Check if the user is an adult.
    pub fn is_adult(&self) -> bool {
        self.age >= 18
    }

    fn internal_helper(&self) -> String {
        self.name.to_lowercase()
    }
}

impl Validate for User {
    fn is_valid(&self) -> bool {
        !self.name.is_empty() && self.age < 200
    }
}

impl Summary for User {
    fn summarize(&self) -> String {
        format!("{} (age {})", self.name, self.age)
    }
}

/// Process a batch of users.
pub fn process_users(users: &[User]) -> Vec<String> {
    users.iter().map(|u| u.summarize()).collect()
}

/// A duplicate symbol name (also exists in utils/helpers.rs).
pub fn helper() -> &'static str {
    "services helper"
}
