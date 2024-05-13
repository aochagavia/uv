//! Find requested Python interpreters and query interpreters for information.
use thiserror::Error;

pub use crate::discovery::{
    find_best_interpreter, find_default_interpreter, find_interpreter, Error as DiscoveryError,
    InterpreterNotFound, InterpreterRequest, InterpreterSource, SourceSelector, SystemPython,
    VersionRequest,
};
pub use crate::environment::PythonEnvironment;
pub use crate::interpreter::Interpreter;
pub use crate::python_version::PythonVersion;
pub use crate::target::Target;
pub use crate::virtualenv::{Error as VirtualEnvError, PyVenvConfiguration, VirtualEnvironment};

mod discovery;
mod environment;
mod implementation;
mod interpreter;
pub mod managed;
pub mod platform;
mod py_launcher;
mod python_version;
mod target;
mod virtualenv;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    VirtualEnv(#[from] virtualenv::Error),

    #[error(transparent)]
    Query(#[from] interpreter::Error),

    #[error(transparent)]
    Discovery(#[from] discovery::Error),

    #[error(transparent)]
    PyLauncher(#[from] py_launcher::Error),

    #[error(transparent)]
    NotFound(#[from] discovery::InterpreterNotFound),
}
