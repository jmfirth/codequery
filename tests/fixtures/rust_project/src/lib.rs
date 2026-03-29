//! Fixture project for codequery integration tests.

pub mod models;
pub mod traits;
pub mod services;
mod utils;

/// A public function at the crate root.
pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

/// Entry point for testing same-file resolution.
pub fn run() {
    let msg = greet("world");
    println!("{msg}");
}

/// Configuration constant.
pub const MAX_RETRIES: u32 = 3;

static INSTANCE_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
