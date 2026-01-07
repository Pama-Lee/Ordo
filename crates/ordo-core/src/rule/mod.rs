//! Rule module
//!
//! Provides rule models and step flow execution, including:
//! - RuleSet definition
//! - Step flow model (Decision Step, Action Step, Terminal Step)
//! - Condition and branch definitions

mod model;
mod step;
mod executor;

pub use model::{RuleSet, RuleSetConfig};
pub use step::{Step, StepKind, Branch, Condition, Action, ActionKind, TerminalResult};
pub use executor::RuleExecutor;

