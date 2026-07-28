use thiserror::Error;

/// Stable error code emitted by the ASCII export pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsciiExportErrorCode {
    InvalidInput,
    UnsupportedEncoding,
    InvalidSequence,
    Io,
}

/// Standardized failure type for ASCII export validation and normalization.
#[derive(Debug, Error)]
pub enum AsciiExportError {
    #[error("ASCII_INVALID_INPUT: unsupported input payload")]
    InvalidInput,
    #[error("ASCII_UNSUPPORTED_ENCODING: {0}")]
    UnsupportedEncoding(String),
    #[error("ASCII_INVALID_SEQUENCE: {0}")]
    InvalidSequence(String),
    #[error("ASCII_IO: {0}")]
    Io(String),
}

impl AsciiExportError {
    /// Maps the detailed error variant to a stable public error code.
    pub fn code(&self) -> AsciiExportErrorCode {
        match self {
            Self::InvalidInput => AsciiExportErrorCode::InvalidInput,
            Self::UnsupportedEncoding(_) => AsciiExportErrorCode::UnsupportedEncoding,
            Self::InvalidSequence(_) => AsciiExportErrorCode::InvalidSequence,
            Self::Io(_) => AsciiExportErrorCode::Io,
        }
    }
}
