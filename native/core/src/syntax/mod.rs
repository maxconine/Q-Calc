//! The expression language: mathjs syntax, which every text pass in the engine rewrites into.

pub mod ast;
pub mod lexer;
pub mod parser;

pub use ast::{Constant, Node, Op};
pub use parser::parse;

/// A syntax error with mathjs's message and 1-based character position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub char: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}
