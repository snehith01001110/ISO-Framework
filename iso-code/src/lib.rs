#![doc = include_str!("../README.md")]

pub mod adapter;
pub mod error;
pub mod git;
pub mod guards;
pub mod lock;
pub mod manager;
pub mod platform;
pub mod ports;
pub mod state;
pub mod types;
pub mod util;

// Re-export all public types at the crate root
pub use adapter::{EcosystemAdapter, SetupContext};
pub use error::WorktreeError;
pub use manager::Manager;
pub use types::{
    AttachOptions, Config, CopyOutcome, CreateOptions, DeleteOptions, GcOptions, GcReport,
    GitCapabilities, GitCryptStatus, GitVersion, PortLease, ReflinkMode, WorktreeHandle,
    WorktreeState,
};
