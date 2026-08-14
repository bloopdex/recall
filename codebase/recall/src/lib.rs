//! Recall — personal engineering solution memory (library crate).
//!
//! Module layout follows the BloopLab architecture principles: the domain
//! model owns no infrastructure, the database/git modules are behind clear
//! interfaces, and the CLI is a thin adapter over the application layer.

pub mod application;
pub mod cli;
pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod observability;

pub use error::{Error, Result};
