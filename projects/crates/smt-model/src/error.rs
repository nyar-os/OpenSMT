use std::fmt;

use serde::Serialize;

/// A stable, machine-readable error.
///
/// `code` is the contract used by the CLI and by callers; `message` is meant for
/// humans and may change. Exit code conventions of the `osmt` binary are:
/// `0` success, `1` input/validation/IO error, `2` capability not implemented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtError {
    /// Stable error code such as `component/outside-board`.
    pub code: String,
    /// Human readable explanation.
    pub message: String,
}

impl SmtError {
    /// Creates an error with a stable code and a human readable message.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }

    /// Returns the process exit code that corresponds to this error.
    pub fn exit_code(&self) -> i32 {
        if self.code.starts_with("capability/") { 2 } else { 1 }
    }
}

impl fmt::Display for SmtError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SmtError {}

/// Builds the stable error used for every declared-but-unimplemented route.
pub fn not_implemented(feature: &str) -> SmtError {
    SmtError::new("capability/not-implemented", format!("`{feature}` is not implemented yet"))
}
