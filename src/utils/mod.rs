// src/utils/mod.rs
pub mod error;
pub mod logging;
pub mod html_debug;

pub use error::AppError; // Re-export main error type for convenience