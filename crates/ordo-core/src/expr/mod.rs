//! Expression module
//!
//! Provides expression parsing and evaluation, including:
//! - Expression AST (Expr)
//! - Expression parser
//! - Expression evaluator
//! - Built-in functions

mod ast;
mod parser;
mod eval;
mod functions;

pub use ast::{Expr, BinaryOp, UnaryOp};
pub use parser::ExprParser;
pub use eval::Evaluator;
pub use functions::FunctionRegistry;

