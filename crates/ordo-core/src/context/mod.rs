//! Execution context module
//!
//! Provides context management during rule execution, including:
//! - Value type system (Value)
//! - Context storage (Context)

mod value;
mod store;

pub use value::Value;
pub use store::Context;
