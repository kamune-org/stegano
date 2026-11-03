use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unsupported carrier format: {0}")]
    UnsupportedFormat(String),

    #[error("this carrier is not suitable for hiding data: {0}")]
    UnsuitableCarrier(String),

    #[error("could not read carrier: {0}")]
    Decode(String),

    #[error("could not write carrier: {0}")]
    Encode(String),

    #[error("payload is too large: {needed} bytes needed, {available} bytes available")]
    PayloadTooLarge { needed: usize, available: usize },

    /// Deliberately the same error for "nothing hidden here" and "wrong passphrase",
    /// so the app never confirms that a carrier holds data.
    #[error("no hidden data found, or the passphrase is wrong")]
    NotFound,

    #[error("hidden data is corrupt: {0}")]
    Corrupt(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("internal error: {0}")]
    Internal(String),
}
