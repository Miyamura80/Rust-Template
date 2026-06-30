//! Engine crate – shared backend logic for the Tauri template app.
//!
//! This crate contains all real backend logic and OS integrations behind
//! traits. It does NOT depend on Tauri runtime types, so it can be used
//! by both the GUI wrapper and the headless CLI test harness.

pub mod commands;
pub mod context;
pub mod doctor;
pub mod platform;
pub mod probes;
pub mod scenario;
pub mod traits;
pub mod types;

// Re-exports for convenience
pub use commands::CommandRegistry;
pub use context::AppContext;
pub use types::{CommandResult, ErrorCode, ErrorInfo, Status};
