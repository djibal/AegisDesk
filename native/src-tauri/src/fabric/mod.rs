//! Intelligence Fabric: on-device threat scoring and permission decisions.
//!
//! * 100% local: a compact logistic model (JSON weights, ~2 KB) bundled with
//!   the app. Updates are optional signed-file drops; no cloud calls.
//! * Cheap: scoring is pure arithmetic over string features (microseconds),
//!   results are cached, and background work runs on one low-priority thread
//!   with a bounded queue that drops work instead of stalling the UI.

pub mod features;
pub mod model;
pub mod permissions;
pub mod worker;

pub use model::{Model, ThreatScore};
pub use permissions::{Decision, PermissionAdvisor, PermissionRequest};
pub use worker::Fabric;
