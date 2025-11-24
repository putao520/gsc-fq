//! Cross-platform system service management module
//!
//! This module provides unified API for managing GSC-FQ as a system service
//! across different platforms (Windows, Linux, macOS).

pub mod manager;
pub mod cli;

pub use manager::*;
pub use cli::*;