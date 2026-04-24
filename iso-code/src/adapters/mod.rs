//! Built-in [`EcosystemAdapter`](crate::adapter::EcosystemAdapter) implementations.
//!
//! Each submodule defines one adapter. Downstream users can also implement the
//! trait themselves and register the result via
//! [`Manager::with_adapter`](crate::Manager::with_adapter).

pub mod default;
pub mod shell_command;

pub use default::DefaultAdapter;
pub use shell_command::ShellCommandAdapter;
