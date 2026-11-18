//! Dedicated live subscription CLI, intentionally outside the generic summary
//! provider resolver. Its failure can never select a paid API or another model.
pub mod cli;
pub mod commands;
pub mod context;
pub mod jobs;
pub mod service;

pub mod vocabulary;
