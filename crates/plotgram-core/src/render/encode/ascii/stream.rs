use std::io::{Cursor, Read, Write};

use super::config::{
    AsciiDetectedEncoding, AsciiExportMetadata, AsciiExportOptions, AsciiExportResult,
    AsciiInputEncodingHint, AsciiInvalidInputPolicy, AsciiNonAsciiPolicy,
};
use super::error::AsciiExportError;

/// Normalizes a UTF-8 string into ASCII-safe output using the configured policy set.
pub fn export_string(
    input: &str,
    options: &AsciiExportOptions,
) -> Result<AsciiExportResult, AsciiExportError> {
    let mut processor = AsciiTextProcessor::new(options, AsciiDetectedEncoding::Utf8);
    processor.metadata.input_bytes = input.len();
    processor.metadata.chunks_processed = 1;

    let mut output = Vec::with_capacity(input.len());
    processor.process_text(input, &mut output, false)?;
    processor.finish(&mut output, false)?;

    Ok(finalize_result(output, processor.metadata, options))
}

/// Normalizes an arbitrary byte payload into ASCII-safe output with encoding detection.
pub fn export_bytes(
    input: &[u8],
    options: &AsciiExportOptions,
) -> Result<AsciiExportResult, AsciiExportError> {
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    let metadata = export_reader_to_writer(&mut reader, &mut output, options)?;
    Ok(finalize_result(output, metadata, options))
}

/// Streams normalized ASCII output from a reader to a writer without buffering the full input.
pub fn export_reader_to_writer<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    options: &AsciiExportOptions,
) -> Result<AsciiExportMetadata, AsciiExportError> {
    let chunk_size = options.normalized_chunk_size();
    let mut first_chunk = vec![0_u8; chunk_size];
    let first_read = reader
        .read(&mut first_chunk)
        .map_err(|err| AsciiExportError::Io(err.to_string()))?;
    first_chunk.truncate(first_read);

    let (detected_encoding, bom_skip) = detect_encoding(&first_chunk, options)?;
    let mut decoder = DecoderState::new(detected_encoding);
    let mut processor = AsciiTextProcessor::new(options, detected_encoding);
    processor.metadata.input_bytes += first_chunk.len();

    if !first_chunk.is_empty() {
        processor.metadata.chunks_processed += 1;
        decoder.process_bytes(&first_chunk[bom_skip..], &mut processor, writer, false)?;
    }

    let mut chunk = vec![0_u8; chunk_size];
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|err| AsciiExportError::Io(err.to_string()))?;
        if read == 0 {
            break;
        }
        processor.metadata.input_bytes += read;
        processor.metadata.chunks_processed += 1;
        decoder.process_bytes(&chunk[..read], &mut processor, writer, false)?;
    }

    decoder.finish(&mut processor, writer)?;
    processor.finish(writer, true)?;
    Ok(processor.metadata)
}

/// Sanitizes an inline fragment so diagram labels stay on a single rendered line.
pub fn sanitize_inline_fragment(
    input: &str,
    options: &AsciiExportOptions,
) -> Result<String, AsciiExportError> {
    let mut processor = AsciiTextProcessor::new(options, AsciiDetectedEncoding::Utf8);
    let mut output = Vec::with_capacity(input.len());
    processor.process_text(input, &mut output, true)?;
    processor.finish(&mut output, false)?;
    String::from_utf8(output).map_err(|err| AsciiExportError::InvalidSequence(err.to_string()))
}

fn finalize_result(
    output: Vec<u8>,
    mut metadata: AsciiExportMetadata,
    options: &AsciiExportOptions,
) -> AsciiExportResult {
    let newline = options.newline_str();
    let mut text = String::from_utf8(output).unwrap_or_default();
    if options.include_metadata {
        if !text.is_empty() {
            text.push_str(newline);
        }
        text.push_str(&metadata_footer(&metadata, options));
    }
    metadata.output_bytes = text.len();
    AsciiExportResult { text, metadata }
}

fn metadata_footer(metadata: &AsciiExportMetadata, options: &AsciiExportOptions) -> String {
    let separator = &options.field_separator;
    let entries = [
        ("detected_input", format!("{:?}", metadata.detected_input_encoding).to_lowercase()),
        ("output_encoding", format!("{:?}", metadata.output_encoding).to_lowercase()),
        ("newline", format!("{:?}", metadata.newline).to_lowercase()),
        ("input_bytes", metadata.input_bytes.to_string()),
        ("output_bytes", metadata.output_bytes.to_string()),
        ("chunks", metadata.chunks_processed.to_string()),
        ("escaped_controls", metadata.escaped_control_chars.to_string()),
        ("escaped_non_ascii", metadata.escaped_non_ascii.to_string()),
        (
            "approximated_non_ascii",
            metadata.approximated_non_ascii.to_string(),
        ),
        ("replaced_non_ascii", metadata.replaced_non_ascii.to_string()),
        ("dropped_non_ascii", metadata.dropped_non_ascii.to_string()),
        (
            "invalid_input_replacements",
            metadata.invalid_input_replacements.to_string(),
        ),
    ];

    let mut line = String::from("# ascii_export");
    for (key, value) in entries {
        line.push_str(separator);
        line.push_str(key);
        line.push('=');
        line.push_str(&value);
    }
    line
}

fn detect_encoding(
    sample: &[u8],
    options: &AsciiExportOptions,
) -> Result<(AsciiDetectedEncoding, usize), AsciiExportError> {
    let explicit = match options.input_encoding {
        AsciiInputEncodingHint::Utf8 => Some((AsciiDetectedEncoding::Utf8, 0)),
        AsciiInputEncodingHint::Utf16Le => Some((AsciiDetectedEncoding::Utf16Le, 0)),
        AsciiInputEncodingHint::Utf16Be => Some((AsciiDetectedEncoding::Utf16Be, 0)),
        AsciiInputEncodingHint::ExtendedAscii => Some((AsciiDetectedEncoding::ExtendedAscii, 0)),
        AsciiInputEncodingHint::Auto => None,
    };
    if let Some(encoding) = explicit {
        return Ok(encoding);
    }

    if sample.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok((AsciiDetectedEncoding::Utf8Bom, 3));
    }
    if sample.starts_with(&[0xFF, 0xFE]) {
        return Ok((AsciiDetectedEncoding::Utf16Le, 2));
    }
    if sample.starts_with(&[0xFE, 0xFF]) {
        return Ok((AsciiDetectedEncoding::Utf16Be, 2));
    }
    if sample.iter().all(u8::is_ascii) {
        return Ok((AsciiDetectedEncoding::Ascii, 0));
    }

    match std::str::from_utf8(sample) {
        Ok(_) => Ok((AsciiDetectedEncoding::Utf8, 0)),
        Err(err) if err.error_len().is_none() => Ok((AsciiDetectedEncoding::Utf8, 0)),
        Err(_) if options.allow_extended_ascii_input => Ok((AsciiDetectedEncoding::ExtendedAscii, 0)),
        Err(_) => Err(AsciiExportError::UnsupportedEncoding(
            "unable to detect UTF-8/UTF-16 input and extended ASCII compatibility is disabled"
                .to_string(),
        )),
    }
}

struct AsciiTextProcessor {
    options: AsciiExportOptions,
    metadata: AsciiExportMetadata,
    pending_cr: bool,
}

impl AsciiTextProcessor {
    fn new(options: &AsciiExportOptions, detected_encoding: AsciiDetectedEncoding) -> Self {
        Self {
            options: options.clone(),
            metadata: AsciiExportMetadata::new(options, detected_encoding),
            pending_cr: false,
        }
    }

    fn process_text<W: Write>(
        &mut self,
        text: &str,
        writer: &mut W,
        inline_mode: bool,
    ) -> Result<(), AsciiExportError> {
        for ch in text.chars() {
            self.process_char(ch, writer, inline_mode)?;
        }
        Ok(())
    }

    fn process_char<W: Write>(
        &mut self,
        ch: char,
        writer: &mut W,
        inline_mode: bool,
    ) -> Result<(), AsciiExportError> {
        if inline_mode {
            return self.write_inline_char(ch, writer);
        }

        if self.pending_cr {
            if ch == '\n' {
                self.write_newline(writer)?;
                self.pending_cr = false;
                return Ok(());
            }
            self.write_newline(writer)?;
            self.pending_cr = false;
        }

        match ch {
            '\r' => {
                self.pending_cr = true;
                Ok(())
            }
            '\n' => self.write_newline(writer),
            _ => self.write_ascii_safe(ch, writer),
        }
    }

    fn write_inline_char<W: Write>(
        &mut self,
        ch: char,
        writer: &mut W,
    ) -> Result<(), AsciiExportError> {
        match ch {
            '\r' => self.write_literal("\\r", writer),
            '\n' => self.write_literal("\\n", writer),
            _ => self.write_ascii_safe(ch, writer),
        }
    }

    fn write_ascii_safe<W: Write>(
        &mut self,
        ch: char,
        writer: &mut W,
    ) -> Result<(), AsciiExportError> {
        if ch.is_ascii() {
            if ch.is_ascii_control() || ch == '\u{7F}' {
                self.metadata.escaped_control_chars += 1;
                return self.write_literal(control_escape(ch), writer);
            }
            return self.write_char(ch, writer);
        }

        match self.options.non_ascii_policy {
            AsciiNonAsciiPolicy::Escape => {
                self.metadata.escaped_non_ascii += 1;
                self.write_literal(&format!("\\u{{{:X}}}", ch as u32), writer)
            }
            AsciiNonAsciiPolicy::Replace => {
                self.metadata.replaced_non_ascii += 1;
                self.write_literal("?", writer)
            }
            AsciiNonAsciiPolicy::Drop => {
                self.metadata.dropped_non_ascii += 1;
                Ok(())
            }
            AsciiNonAsciiPolicy::Approximate => {
                if let Some(mapped) = approximate_unicode(ch) {
                    self.metadata.approximated_non_ascii += 1;
                    self.write_literal(mapped, writer)
                } else {
                    self.metadata.escaped_non_ascii += 1;
                    self.write_literal(&format!("\\u{{{:X}}}", ch as u32), writer)
                }
            }
        }
    }

    fn finish<W: Write>(
        &mut self,
        writer: &mut W,
        append_metadata: bool,
    ) -> Result<(), AsciiExportError> {
        if self.pending_cr {
            self.write_newline(writer)?;
            self.pending_cr = false;
        }

        if append_metadata && self.options.include_metadata {
            self.write_literal(self.options.newline_str(), writer)?;
            let footer = metadata_footer(&self.metadata, &self.options);
            self.write_literal(&footer, writer)?;
        }

        Ok(())
    }

    fn write_newline<W: Write>(&mut self, writer: &mut W) -> Result<(), AsciiExportError> {
        self.write_literal(self.options.newline_str(), writer)
    }

    fn write_char<W: Write>(&mut self, ch: char, writer: &mut W) -> Result<(), AsciiExportError> {
        let mut buf = [0_u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        writer
            .write_all(encoded.as_bytes())
            .map_err(|err| AsciiExportError::Io(err.to_string()))?;
        self.metadata.output_bytes += encoded.len();
        Ok(())
    }

    fn write_literal<W: Write>(
        &mut self,
        value: &str,
        writer: &mut W,
    ) -> Result<(), AsciiExportError> {
        writer
            .write_all(value.as_bytes())
            .map_err(|err| AsciiExportError::Io(err.to_string()))?;
        self.metadata.output_bytes += value.len();
        Ok(())
    }
}

enum DecoderState {
    Ascii,
    Utf8 { pending: Vec<u8> },
    Utf16Le {
        pending_byte: Option<u8>,
        pending_high_surrogate: Option<u16>,
    },
    Utf16Be {
        pending_byte: Option<u8>,
        pending_high_surrogate: Option<u16>,
    },
    ExtendedAscii,
}

impl DecoderState {
    fn new(encoding: AsciiDetectedEncoding) -> Self {
        match encoding {
            AsciiDetectedEncoding::Ascii => Self::Ascii,
            AsciiDetectedEncoding::Utf8 | AsciiDetectedEncoding::Utf8Bom => {
                Self::Utf8 { pending: Vec::new() }
            }
            AsciiDetectedEncoding::Utf16Le => Self::Utf16Le {
                pending_byte: None,
                pending_high_surrogate: None,
            },
            AsciiDetectedEncoding::Utf16Be => Self::Utf16Be {
                pending_byte: None,
                pending_high_surrogate: None,
            },
            AsciiDetectedEncoding::ExtendedAscii => Self::ExtendedAscii,
        }
    }

    fn process_bytes<W: Write>(
        &mut self,
        bytes: &[u8],
        processor: &mut AsciiTextProcessor,
        writer: &mut W,
        is_final: bool,
    ) -> Result<(), AsciiExportError> {
        match self {
            Self::Ascii => {
                let text = String::from_utf8_lossy(bytes);
                processor.process_text(text.as_ref(), writer, false)
            }
            Self::ExtendedAscii => {
                let mut text = String::with_capacity(bytes.len());
                for byte in bytes {
                    text.push(decode_extended_ascii(*byte));
                }
                processor.process_text(&text, writer, false)
            }
            Self::Utf8 { pending } => process_utf8_bytes(pending, bytes, processor, writer, is_final),
            Self::Utf16Le {
                pending_byte,
                pending_high_surrogate,
            } => process_utf16_bytes(
                bytes,
                true,
                pending_byte,
                pending_high_surrogate,
                processor,
                writer,
                is_final,
            ),
            Self::Utf16Be {
                pending_byte,
                pending_high_surrogate,
            } => process_utf16_bytes(
                bytes,
                false,
                pending_byte,
                pending_high_surrogate,
                processor,
                writer,
                is_final,
            ),
        }
    }

    fn finish<W: Write>(
        &mut self,
        processor: &mut AsciiTextProcessor,
        writer: &mut W,
    ) -> Result<(), AsciiExportError> {
        self.process_bytes(&[], processor, writer, true)
    }
}

fn process_utf8_bytes<W: Write>(
    pending: &mut Vec<u8>,
    bytes: &[u8],
    processor: &mut AsciiTextProcessor,
    writer: &mut W,
    is_final: bool,
) -> Result<(), AsciiExportError> {
    pending.extend_from_slice(bytes);
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                processor.process_text(text, writer, false)?;
                pending.clear();
                break;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                if valid_up_to > 0 {
                    let valid = std::str::from_utf8(&pending[..valid_up_to])
                        .map_err(|decode_err| AsciiExportError::InvalidSequence(decode_err.to_string()))?;
                    processor.process_text(valid, writer, false)?;
                    pending.drain(..valid_up_to);
                }

                if let Some(error_len) = err.error_len() {
                    handle_invalid_sequence(processor, writer)?;
                    pending.drain(..error_len);
                    continue;
                }

                if is_final {
                    handle_invalid_sequence(processor, writer)?;
                    pending.clear();
                }
                break;
            }
        }
    }
    Ok(())
}

fn process_utf16_bytes<W: Write>(
    bytes: &[u8],
    little_endian: bool,
    pending_byte: &mut Option<u8>,
    pending_high_surrogate: &mut Option<u16>,
    processor: &mut AsciiTextProcessor,
    writer: &mut W,
    is_final: bool,
) -> Result<(), AsciiExportError> {
    let mut text = String::new();
    for byte in bytes {
        if let Some(first) = pending_byte.take() {
            let unit = if little_endian {
                u16::from_le_bytes([first, *byte])
            } else {
                u16::from_be_bytes([first, *byte])
            };
            decode_utf16_unit(unit, pending_high_surrogate, &mut text, processor)?;
        } else {
            *pending_byte = Some(*byte);
        }
    }

    if !text.is_empty() {
        processor.process_text(&text, writer, false)?;
    }

    if is_final {
        if pending_byte.take().is_some() || pending_high_surrogate.take().is_some() {
            handle_invalid_sequence(processor, writer)?;
        }
    }

    Ok(())
}

fn decode_utf16_unit(
    unit: u16,
    pending_high_surrogate: &mut Option<u16>,
    output: &mut String,
    processor: &mut AsciiTextProcessor,
) -> Result<(), AsciiExportError> {
    if let Some(high) = pending_high_surrogate.take() {
        if (0xDC00..=0xDFFF).contains(&unit) {
            let scalar = 0x10000 + (((high - 0xD800) as u32) << 10) + (unit - 0xDC00) as u32;
            if let Some(ch) = char::from_u32(scalar) {
                output.push(ch);
                return Ok(());
            }
        }
        processor.metadata.invalid_input_replacements += 1;
        output.push('?');
    }

    if (0xD800..=0xDBFF).contains(&unit) {
        *pending_high_surrogate = Some(unit);
        return Ok(());
    }
    if (0xDC00..=0xDFFF).contains(&unit) {
        processor.metadata.invalid_input_replacements += 1;
        output.push('?');
        return Ok(());
    }

    if let Some(ch) = char::from_u32(unit as u32) {
        output.push(ch);
        Ok(())
    } else {
        handle_invalid_codepoint(processor, output)
    }
}

fn handle_invalid_codepoint(
    processor: &mut AsciiTextProcessor,
    output: &mut String,
) -> Result<(), AsciiExportError> {
    match processor.options.invalid_input_policy {
        AsciiInvalidInputPolicy::Error => Err(AsciiExportError::InvalidSequence(
            "encountered an invalid UTF code point".to_string(),
        )),
        AsciiInvalidInputPolicy::Replace => {
            processor.metadata.invalid_input_replacements += 1;
            output.push('?');
            Ok(())
        }
    }
}

fn handle_invalid_sequence<W: Write>(
    processor: &mut AsciiTextProcessor,
    writer: &mut W,
) -> Result<(), AsciiExportError> {
    match processor.options.invalid_input_policy {
        AsciiInvalidInputPolicy::Error => Err(AsciiExportError::InvalidSequence(
            "encountered an invalid byte sequence while decoding input".to_string(),
        )),
        AsciiInvalidInputPolicy::Replace => {
            processor.metadata.invalid_input_replacements += 1;
            processor.write_literal("?", writer)
        }
    }
}

fn control_escape(ch: char) -> &'static str {
    match ch {
        '\0' => "\\0",
        '\x07' => "\\a",
        '\x08' => "\\b",
        '\t' => "\\t",
        '\n' => "\\n",
        '\x0B' => "\\v",
        '\x0C' => "\\f",
        '\r' => "\\r",
        '\u{7F}' => "\\x7F",
        '\x01' => "\\x01",
        '\x02' => "\\x02",
        '\x03' => "\\x03",
        '\x04' => "\\x04",
        '\x05' => "\\x05",
        '\x06' => "\\x06",
        '\x0E' => "\\x0E",
        '\x0F' => "\\x0F",
        '\x10' => "\\x10",
        '\x11' => "\\x11",
        '\x12' => "\\x12",
        '\x13' => "\\x13",
        '\x14' => "\\x14",
        '\x15' => "\\x15",
        '\x16' => "\\x16",
        '\x17' => "\\x17",
        '\x18' => "\\x18",
        '\x19' => "\\x19",
        '\x1A' => "\\x1A",
        '\x1B' => "\\x1B",
        '\x1C' => "\\x1C",
        '\x1D' => "\\x1D",
        '\x1E' => "\\x1E",
        '\x1F' => "\\x1F",
        _ => "?",
    }
}

fn approximate_unicode(ch: char) -> Option<&'static str> {
    match ch {
        '─' | '╌' | '–' | '—' | '−' => Some("-"),
        '│' | '┆' => Some("|"),
        '┌' | '┐' | '└' | '┘' | '╭' | '╮' | '╰' | '╯' | '┼' | '╋' => Some("+"),
        '→' => Some(">"),
        '←' => Some("<"),
        '↑' => Some("^"),
        '↓' => Some("v"),
        '↔' => Some("<>"),
        '…' => Some("..."),
        '•' | '·' => Some("*"),
        '“' | '”' => Some("\""),
        '‘' | '’' => Some("'"),
        '«' | '»' => Some("\""),
        '€' => Some("EUR"),
        '£' => Some("GBP"),
        '¥' => Some("YEN"),
        '\u{A0}' => Some(" "),
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => Some("A"),
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => Some("a"),
        'Æ' => Some("AE"),
        'æ' => Some("ae"),
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => Some("C"),
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => Some("c"),
        'Ð' | 'Ď' | 'Đ' => Some("D"),
        'ð' | 'ď' | 'đ' => Some("d"),
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => Some("E"),
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => Some("e"),
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => Some("G"),
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => Some("g"),
        'Ĥ' | 'Ħ' => Some("H"),
        'ĥ' | 'ħ' => Some("h"),
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => Some("I"),
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => Some("i"),
        'Ĵ' => Some("J"),
        'ĵ' => Some("j"),
        'Ķ' => Some("K"),
        'ķ' => Some("k"),
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => Some("L"),
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => Some("l"),
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' => Some("N"),
        'ñ' | 'ń' | 'ņ' | 'ň' => Some("n"),
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => Some("O"),
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => Some("o"),
        'Œ' => Some("OE"),
        'œ' => Some("oe"),
        'Ŕ' | 'Ŗ' | 'Ř' => Some("R"),
        'ŕ' | 'ŗ' | 'ř' => Some("r"),
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'ß' => Some("S"),
        'ś' | 'ŝ' | 'ş' | 'š' => Some("s"),
        'Ţ' | 'Ť' | 'Ŧ' => Some("T"),
        'ţ' | 'ť' | 'ŧ' => Some("t"),
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => Some("U"),
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => Some("u"),
        'Ŵ' => Some("W"),
        'ŵ' => Some("w"),
        'Ý' | 'Ŷ' | 'Ÿ' => Some("Y"),
        'ý' | 'ÿ' | 'ŷ' => Some("y"),
        'Ź' | 'Ż' | 'Ž' => Some("Z"),
        'ź' | 'ż' | 'ž' => Some("z"),
        _ => None,
    }
}

fn decode_extended_ascii(byte: u8) -> char {
    match byte {
        0x80 => '€',
        0x82 => '‚',
        0x83 => 'ƒ',
        0x84 => '„',
        0x85 => '…',
        0x86 => '†',
        0x87 => '‡',
        0x88 => 'ˆ',
        0x89 => '‰',
        0x8A => 'Š',
        0x8B => '‹',
        0x8C => 'Œ',
        0x8E => 'Ž',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x98 => '˜',
        0x99 => '™',
        0x9A => 'š',
        0x9B => '›',
        0x9C => 'œ',
        0x9E => 'ž',
        0x9F => 'Ÿ',
        _ => char::from(byte),
    }
}
