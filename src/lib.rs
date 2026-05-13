mod ast;
mod builtins;
mod env;
mod error;
mod format;
mod lexer;
mod parser;
mod runtime;
mod span;
mod token;
mod typecheck;
mod value;
mod vm;

pub use error::{BoryError, BoryErrorKind};
pub use format::{format_program, format_source};
pub use runtime::{Interpreter, check_source};
pub use typecheck::check_program;
pub use value::Value;

use std::path::Path;

pub fn run_source(source: &str, source_name: &str) -> Result<Value, BoryError> {
    let mut interpreter = Interpreter::new();
    interpreter.run_source(source, source_name)
}

pub fn run_file(path: impl AsRef<Path>) -> Result<Value, BoryError> {
    let mut interpreter = Interpreter::new();
    interpreter.run_file(path.as_ref())
}
