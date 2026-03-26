use std::collections::HashMap;

/// A configuration holder.
pub struct Config {
    pub name: String,
    pub max_retries: u32,
}

impl Config {
    /// Create a new config with defaults.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            max_retries: 3,
        }
    }
}

/// Greet a user by name.
pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

/// Maximum retry count.
pub const MAX_RETRIES: u32 = 5;

fn main() {
    let cfg = Config::new("default");
    println!("{}", greet(&cfg.name));
}
