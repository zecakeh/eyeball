//! Add observability to your Rust types!
//!
//! Cargo features:
//!
//! - `tracing`: Emit [tracing] events when updates are sent out
#![warn(missing_debug_implementations, missing_docs)]
#![allow(clippy::new_without_default)]

mod observable;

pub use observable::{Observable, Subscriber};
