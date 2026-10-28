//! Intelligent Resource Allocation. Every 5 s (one low-priority thread):
//! 1. Attribute WebView2 processes to apps via their --user-data-dir.
//! 2. Learn each app's normal usage (EWMA of RAM and CPU).
//! 3. Derive a dynamic limit = learned peak + headroom, clamped.
//! 4. Enforce: over CPU -> lower priority; over RAM -> trim working set;
//!    idle in background too long -> suspend (window destroyed, session
//!    kept on disk, restored on next open).

pub mod enforce;
pub mod policy;

pub use policy::{AppUsage, Limits};
