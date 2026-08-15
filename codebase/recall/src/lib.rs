//! Recall — personal engineering solution memory (library crate).
//!
//! Module layout: the domain model owns no infrastructure, the
//! database/git modules sit behind clear interfaces, and the CLI is a
//! thin adapter over the application layer.

pub mod application;
pub mod cli;
pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod observability;
pub mod ui;

pub use error::{Error, Result};
