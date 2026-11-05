//! Command errors as `{ code, message }` for the frontend.

use serde::Serialize;
use stegano_core::Error as CoreError;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

pub type CmdResult<T> = Result<T, CommandError>;

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let code = match self {
            Self::Core(err) => core_code(err),
            Self::Io(_) => "io",
            Self::Other(_) => "other",
        };
        ErrorBody {
            code,
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

/// Decode and encode failures stay `"other"`. Only [`CommandError::Io`] is `"io"`.
fn core_code(err: &CoreError) -> &'static str {
    match err {
        CoreError::Cancelled => "cancelled",
        CoreError::NotFound => "not_found",
        CoreError::PayloadTooLarge { .. } => "too_large",
        CoreError::UnsuitableCarrier(_) => "unsuitable",
        CoreError::UnsupportedFormat(_) => "unsupported",
        CoreError::Corrupt(_) => "corrupt",
        CoreError::Decode(_)
        | CoreError::Encode(_)
        | CoreError::InvalidPayload(_)
        | CoreError::Internal(_) => "other",
    }
}
