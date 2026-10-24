//! BrotliDecode filter (PDF 2.0, ISO 32000-2:2020).

use crate::error::{Error, Result};
use std::io::Read;

use super::{flate::effective_limit, StreamDecoder};

// The Brotli reader enables the non-standard large-window extension, which can
// allocate a 1 GiB ring buffer before producing any output. PDF BrotliDecode
// uses the standard format, whose largest window is 16 MiB.
const LARGE_WINDOW_HEADER: u8 = 0x11;

fn decode_with_limit(input: &[u8], max_decompressed_bytes: u64) -> Result<Vec<u8>> {
    if input.first() == Some(&LARGE_WINDOW_HEADER) {
        return Err(Error::Decode(
            "BrotliDecode large-window extension is unsupported".to_string(),
        ));
    }

    let mut output = Vec::new();
    let reader = brotli::Decompressor::new(input, 4096);
    reader
        .take(max_decompressed_bytes.saturating_add(1))
        .read_to_end(&mut output)
        .map_err(|e| Error::Decode(format!("BrotliDecode: {e}")))?;
    if output.len() as u64 > max_decompressed_bytes {
        return Err(Error::Decode(format!(
            "BrotliDecode output exceeds the {} MB safety limit",
            max_decompressed_bytes / (1024 * 1024)
        )));
    }
    Ok(output)
}

/// Decoder for BrotliDecode filter (PDF 2.0).
pub struct BrotliDecoder;

impl StreamDecoder for BrotliDecoder {
    fn decode(&self, input: &[u8]) -> Result<Vec<u8>> {
        decode_with_limit(input, effective_limit())
    }

    fn name(&self) -> &str {
        "BrotliDecode"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compress(input: &[u8]) -> Vec<u8> {
        let mut compressed = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 6, 22);
            std::io::Write::write_all(&mut writer, input).unwrap();
        }
        compressed
    }

    #[test]
    fn test_brotli_roundtrip() {
        let original = b"Hello, PDF 2.0 BrotliDecode filter! This is a test.";

        // Compress with brotli
        let compressed = compress(original);

        // Decompress with our decoder
        let decoder = BrotliDecoder;
        let decoded = decoder.decode(&compressed).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_brotli_empty_input() {
        // Brotli-compressed empty data
        let compressed = compress(b"");
        let decoder = BrotliDecoder;
        let decoded = decoder.decode(&compressed).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_brotli_output_limit() {
        let original = vec![b'A'; 16 * 1024];
        let compressed = compress(&original);
        assert!(compressed.len() < original.len());

        assert_eq!(decode_with_limit(&compressed, original.len() as u64).unwrap(), original);
        let error = decode_with_limit(&compressed, original.len() as u64 - 1).unwrap_err();
        assert!(format!("{error}").contains("safety limit"));
    }

    #[test]
    fn test_brotli_rejects_large_window_header() {
        let error = decode_with_limit(&[LARGE_WINDOW_HEADER], 1024).unwrap_err();
        assert!(format!("{error}").contains("large-window"));
    }
}
