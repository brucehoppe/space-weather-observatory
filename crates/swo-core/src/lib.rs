//! Space Weather Observatory — scientific core.
//!
//! This crate holds every rule that decides what a number *means*: parsing,
//! normalization, time alignment, aggregation, flux classification, the
//! solar-wind alert state machine, the interpretation rule layer and export.
//!
//! It performs no I/O and reads no clock of its own — `now` is always passed
//! in — so all of it is deterministically testable against captured originals.

pub mod aggregate;
pub mod alert;
pub mod export;
pub mod flare;
pub mod interpret;
pub mod model;
pub mod parse;
pub mod timeline;

/// Schema version of the local cache and stored settings.
pub const SCHEMA_VERSION: u32 = 1;
