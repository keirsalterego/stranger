#![forbid(unsafe_code)]

//! stranger — audit a dependency tree without installing, resolving, or
//! phoning anything.
//!
//! The library half exists so the integration tests in `tests/` can drive the
//! parsers directly instead of shelling out to the binary and grepping stdout.

pub mod distance;
pub mod error;
pub mod json;
pub mod lock;
pub mod toml;
