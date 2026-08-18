use serde::{Deserialize, Serialize};

const DEFAULT_CHUNK_SIZE: usize = 8 * 1024;
const MIN_CHUNK_SIZE: usize = 256;

/// Input decoding hint for ASCII export normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsciiInputEncodingHint {
    Auto,
    Utf8,
    Utf16Le,
    Utf16Be,
    ExtendedAscii,
}

/// Detected source encoding after BOM and byte-pattern inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsciiDetectedEncoding {
    Ascii,
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    ExtendedAscii,
}

/// Normalized newline sequence used by exported text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsciiNewline {
    Lf,
    Crlf,
}

/// Declares the target text encoding contract for downstream callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsciiOutputEncoding {
    Ascii,
    Utf8,
}

/// Behavior used when the input payload cannot be decoded cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsciiInvalidInputPolicy {
    Error,
    Replace,
}

/// Strategy used when characters fall outside the ASCII range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsciiNonAsciiPolicy {
    Escape,
    Replace,
    Drop,
    Approximate,
}

/// Configures ASCII export normalization, escaping, and streaming behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsciiExportOptions {
    #[serde(default)]
    pub input_encoding: AsciiInputEncodingHint,
    #[serde(default)]
    pub output_encoding: AsciiOutputEncoding,
    #[serde(default)]
    pub newline: AsciiNewline,
    #[serde(default = "default_field_separator")]
    pub field_separator: String,
    #[serde(default)]
    pub invalid_input_policy: AsciiInvalidInputPolicy,
    #[serde(default)]
    pub non_ascii_policy: AsciiNonAsciiPolicy,
    #[serde(default = "default_true")]
    pub allow_extended_ascii_input: bool,
    #[serde(default)]
    pub include_metadata: bool,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
}

/// Captures encoding and sanitization statistics for one export operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsciiExportMetadata {
    pub detected_input_encoding: AsciiDetectedEncoding,
    pub output_encoding: AsciiOutputEncoding,
    pub newline: AsciiNewline,
    pub field_separator: String,
    pub chunk_size: usize,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub chunks_processed: usize,
    pub escaped_control_chars: usize,
    pub escaped_non_ascii: usize,
    pub approximated_non_ascii: usize,
    pub replaced_non_ascii: usize,
    pub dropped_non_ascii: usize,
    pub invalid_input_replacements: usize,
    pub metadata_appended: bool,
}

/// Final text payload plus structured export metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsciiExportResult {
    pub text: String,
    pub metadata: AsciiExportMetadata,
}

impl AsciiExportOptions {
    /// Returns the canonical newline sequence for the current export options.
    pub fn newline_str(&self) -> &'static str {
        match self.newline {
            AsciiNewline::Lf => "\n",
            AsciiNewline::Crlf => "\r\n",
        }
    }

    /// Clamps the streaming chunk size to a safe lower bound.
    pub fn normalized_chunk_size(&self) -> usize {
        self.chunk_size.max(MIN_CHUNK_SIZE)
    }
}

impl Default for AsciiExportOptions {
    fn default() -> Self {
        Self {
            input_encoding: AsciiInputEncodingHint::Auto,
            output_encoding: AsciiOutputEncoding::Utf8,
            newline: AsciiNewline::Lf,
            field_separator: default_field_separator(),
            invalid_input_policy: AsciiInvalidInputPolicy::Replace,
            non_ascii_policy: AsciiNonAsciiPolicy::Approximate,
            allow_extended_ascii_input: true,
            include_metadata: false,
            chunk_size: default_chunk_size(),
        }
    }
}

impl Default for AsciiInputEncodingHint {
    fn default() -> Self {
        Self::Auto
    }
}

impl Default for AsciiOutputEncoding {
    fn default() -> Self {
        Self::Utf8
    }
}

impl Default for AsciiNewline {
    fn default() -> Self {
        Self::Lf
    }
}

impl Default for AsciiInvalidInputPolicy {
    fn default() -> Self {
        Self::Replace
    }
}

impl Default for AsciiNonAsciiPolicy {
    fn default() -> Self {
        Self::Approximate
    }
}

impl AsciiExportMetadata {
    pub fn new(options: &AsciiExportOptions, detected_input_encoding: AsciiDetectedEncoding) -> Self {
        Self {
            detected_input_encoding,
            output_encoding: options.output_encoding,
            newline: options.newline,
            field_separator: options.field_separator.clone(),
            chunk_size: options.normalized_chunk_size(),
            input_bytes: 0,
            output_bytes: 0,
            chunks_processed: 0,
            escaped_control_chars: 0,
            escaped_non_ascii: 0,
            approximated_non_ascii: 0,
            replaced_non_ascii: 0,
            dropped_non_ascii: 0,
            invalid_input_replacements: 0,
            metadata_appended: options.include_metadata,
        }
    }
}

fn default_chunk_size() -> usize {
    DEFAULT_CHUNK_SIZE
}

fn default_field_separator() -> String {
    " | ".to_string()
}

fn default_true() -> bool {
    true
}
