use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AegisError {
    #[error("invalid URL")]
    InvalidUrl,
    #[error("only HTTPS sites can be wrapped")]
    InsecureScheme,
    #[error("host is not allowed")]
    ForbiddenHost,
    #[error("app not found")]
    NotFound,
    #[error("invalid network policy")]
    InvalidPolicy,
    #[error("storage error")]
    Storage,
    #[error("window error")]
    Window,
}

impl From<std::io::Error> for AegisError {
    fn from(_: std::io::Error) -> Self { AegisError::Storage }
}
impl From<serde_json::Error> for AegisError {
    fn from(_: serde_json::Error) -> Self { AegisError::Storage }
}
impl From<tauri::Error> for AegisError {
    fn from(_: tauri::Error) -> Self { AegisError::Window }
}

// Errors cross the IPC boundary as plain strings; no internals are leaked.
impl Serialize for AegisError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
