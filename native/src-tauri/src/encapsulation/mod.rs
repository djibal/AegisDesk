//! Encapsulation Core: every wrapped site runs in its own window with its
//! own storage partition, zero IPC access, and an optional network policy.

pub mod network;
pub mod registry;
pub mod session;

pub use network::NetworkPolicy;
pub use registry::{Registry, WrappedApp};
