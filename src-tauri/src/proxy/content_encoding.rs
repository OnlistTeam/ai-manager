//! HTTP content-encoding helpers.
//!
//! reqwest's automatic decompression is disabled (to pass accept-encoding through), so
//! decompression is manual here. The request side (e.g. Codex Desktop sending a compressed body
//! while signed in) and the response side (compressed upstream bodies) share this logic.

use axum::http::header::HeaderMap;
use std::io::Read;

/// Splits a content-encoding value into an ordered coding list (dropping identity and empties).
///
/// HTTP allows stacked encodings (e.g. `gzip, zstd`) separated by commas, and also allows
/// repeated content-encoding headers, which mean the same as a comma join (see [`get_content_encoding`]).
fn split_codings(content_encoding: &str) -> Vec<&str> {
    content_encoding
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty() && *c != "identity")
        .collect()
}

/// Whether a single coding can be decompressed.
fn is_single_supported(coding: &str) -> bool {
    matches!(
        coding,
        "gzip" | "x-gzip" | "deflate" | "br" | "zstd" | "zst"
    )
}

/// Why decompression failed. "Output over budget" is kept distinct from "corrupt data": the former
/// is a safety rejection signal, so response-side callers should reject with 502 instead of silently falling back.
#[derive(Debug)]
pub(crate) enum DecompressError {
    /// The underlying decoder failed (corrupt data or wrong format).
    Io(std::io::Error),
    /// Aborted once output exceeded `limit` bytes; the true output size is unknown, only known to be larger.
    TooLarge { limit: usize },
}

impl std::fmt::Display for DecompressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::TooLarge { limit } => {
                write!(f, "decompressed output exceeds the {limit} byte limit")
            }
        }
    }
}

impl std::error::Error for DecompressError {}

impl From<std::io::Error> for DecompressError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<DecompressError> for std::io::Error {
    fn from(e: DecompressError) -> Self {
        match e {
            DecompressError::Io(e) => e,
            DecompressError::TooLarge { limit } => {
                std::io::Error::other(format!("decompressed body exceeds {limit} bytes"))
            }
        }
    }
}

/// Reads decompressed output from the decoder, up to `max_bytes`; once the output exceeds the
/// budget it aborts immediately and returns [`DecompressError::TooLarge`], so a compression bomb
/// is cut off where the budget runs out rather than fully expanded in memory and then measured.
fn read_with_output_limit<R: Read>(
    reader: R,
    max_bytes: usize,
) -> Result<Vec<u8>, DecompressError> {
    // saturating_add: for unbounded calls (max_bytes = usize::MAX) the budget stays usize::MAX
    let budget = max_bytes.saturating_add(1) as u64;
    let mut limited = reader.take(budget);
    let mut out = Vec::new();
    limited.read_to_end(&mut out)?;
    if out.len() > max_bytes {
        return Err(DecompressError::TooLarge { limit: max_bytes });
    }
    Ok(out)
}

/// Decompresses a single content-coding with an output cap of `max_output_bytes`. Unknown codings return `Ok(None)`.
fn decompress_single(
    coding: &str,
    body: &[u8],
    max_output_bytes: usize,
) -> Result<Option<Vec<u8>>, DecompressError> {
    match coding {
        "gzip" | "x-gzip" => {
            let decoder = flate2::read::GzDecoder::new(body);
            Ok(Some(read_with_output_limit(decoder, max_output_bytes)?))
        }
        "deflate" => {
            // RFC 9110: deflate means the zlib wrapper format, but some upstreams/clients send raw deflate.
            // Try zlib per spec first and fall back to raw, otherwise spec-compliant sources always fail and
            // the raw compressed bytes fail open into the JSON parser (one of the #2234 shapes, variant C).
            let zlib = flate2::read::ZlibDecoder::new(body);
            match read_with_output_limit(zlib, max_output_bytes) {
                Ok(decompressed) => Ok(Some(decompressed)),
                Err(zlib_err) => {
                    // TooLarge must also fall back: a raw stream misread as zlib can stop at the budget, and if it
                    // really is a bomb the raw decoder will trigger TooLarge just the same.
                    log::debug!("deflate as zlib failed ({zlib_err}), falling back to raw deflate");
                    let raw = flate2::read::DeflateDecoder::new(body);
                    Ok(Some(read_with_output_limit(raw, max_output_bytes)?))
                }
            }
        }
        "br" => {
            let decoder = brotli::Decompressor::new(std::io::Cursor::new(body), 4096);
            Ok(Some(read_with_output_limit(decoder, max_output_bytes)?))
        }
        "zstd" | "zst" => {
            // Signed-in Codex enables zstd on request bodies (Compression::Zstd); upstreams may zstd responses too.
            let decoder = zstd::stream::read::Decoder::new(std::io::Cursor::new(body))?;
            Ok(Some(read_with_output_limit(decoder, max_output_bytes)?))
        }
        _ => Ok(None),
    }
}

/// Decompresses body bytes according to content-encoding, supporting stacked encodings
/// (e.g. `gzip, zstd`), where every coding's output (including intermediate results of a stack) is
/// capped by `max_output_bytes` and aborts with [`DecompressError::TooLarge`], guarding against response-side compression bombs.
///
/// RFC 9110 section 8.4: codings are listed in **application order**, so decoding must run **in reverse**.
/// `Ok(None)` means an unsupported encoding was present and the body passes through untouched; the
/// caller must then keep the content-encoding header or downstream consumers mistake compressed bytes for plaintext.
pub(crate) fn decompress_body_with_limit(
    content_encoding: &str,
    body: &[u8],
    max_output_bytes: usize,
) -> Result<Option<Vec<u8>>, DecompressError> {
    let codings = split_codings(content_encoding);
    if codings.is_empty() {
        return Ok(None);
    }
    // If any coding is unsupported, skip decompression entirely and pass through with the header, avoiding half-decoded data.
    if !codings.iter().all(|c| is_single_supported(c)) {
        log::warn!("unsupported content-encoding: {content_encoding}, skipping decompression");
        return Ok(None);
    }

    // Decode in reverse: the last list entry was applied last, so it must be undone first.
    let mut data: Option<Vec<u8>> = None;
    for coding in codings.iter().rev() {
        let input = data.as_deref().unwrap_or(body);
        match decompress_single(coding, input, max_output_bytes)? {
            Some(decompressed) => data = Some(decompressed),
            // is_single_supported above already checked this, so it should be unreachable; defensive fallback.
            None => return Ok(None),
        }
    }
    Ok(data)
}

/// An unbounded variant of [`decompress_body_with_limit`], for callers such as the request side
/// that already enforce their own size limits.
pub(crate) fn decompress_body(
    content_encoding: &str,
    body: &[u8],
) -> Result<Option<Vec<u8>>, std::io::Error> {
    decompress_body_with_limit(content_encoding, body, usize::MAX).map_err(Into::into)
}

/// Whether this content-encoding (including stacks such as `gzip, zstd`) is fully decompressible.
///
/// The request side gates on this: an undecompressable body must be rejected, not passed to the JSON parser.
pub(crate) fn is_supported_content_encoding(content_encoding: &str) -> bool {
    let codings = split_codings(content_encoding);
    !codings.is_empty() && codings.iter().all(|c| is_single_supported(c))
}

/// Extracts content-encoding from headers (merging duplicates, ignoring identity and empties).
///
/// HTTP allows repeated content-encoding headers, equivalent to a comma join, hence `get_all`;
/// the result may hold several comma-separated codings, which [`decompress_body`] decodes in reverse.
pub(crate) fn get_content_encoding(headers: &HeaderMap) -> Option<String> {
    let combined = headers
        .get_all("content-encoding")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
        .to_lowercase();
    if split_codings(&combined).is_empty() {
        return None;
    }
    Some(combined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn decompress_body_deflate_handles_zlib_wrapped_per_rfc9110() {
        // RFC 9110 deflate = the zlib wrapper format (what compliant sources send)
        let payload = br#"{"ok":true}"#;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let decompressed = decompress_body("deflate", &compressed).unwrap().unwrap();
        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompress_body_deflate_falls_back_to_raw_stream() {
        // Some sources send raw deflate streams in violation of the spec; stay compatible
        let payload = br#"{"ok":true}"#;
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let decompressed = decompress_body("deflate", &compressed).unwrap().unwrap();
        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompress_body_zstd_roundtrip() {
        // Signed-in Codex sends exactly this: a zstd-compressed request body
        let payload = br#"{"hello":"world","n":42}"#;
        let compressed = zstd::stream::encode_all(std::io::Cursor::new(&payload[..]), 0).unwrap();
        let decompressed = decompress_body("zstd", &compressed).unwrap().unwrap();
        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompress_body_stacked_gzip_then_zstd_decodes_in_reverse() {
        // Content-Encoding: gzip, zstd means gzip then zstd, so decode in reverse (zstd then gzip)
        let payload = br#"{"stacked":true}"#;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, payload).unwrap();
        let gzipped = gz.finish().unwrap();
        let stacked = zstd::stream::encode_all(std::io::Cursor::new(&gzipped[..]), 0).unwrap();

        let decompressed = decompress_body("gzip, zstd", &stacked).unwrap().unwrap();
        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompress_body_stacked_with_unsupported_returns_none() {
        // One unsupported entry in the stack means the whole body passes through with its header
        let result = decompress_body("snappy, zstd", b"\x00\x01\x02\x03").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn decompress_body_unknown_encoding_returns_none_to_keep_headers() {
        // An unknown encoding must return None rather than pretend it decoded, otherwise the
        // content-encoding header is stripped and downstream diagnostics report compressed bytes as plaintext
        let result = decompress_body("snappy", b"\x00\x01\x02\x03").unwrap();
        assert!(result.is_none());
    }

    /// Generates deterministic pseudo-random bytes (LCG) so tests need no rand dependency.
    fn pseudo_random_bytes(len: usize) -> Vec<u8> {
        let mut state: u64 = 0x243F_6A88_85A3_08D3;
        (0..len)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (state >> 33) as u8
            })
            .collect()
    }

    fn gzip_compress(payload: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, payload).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn decompress_body_with_limit_passes_payload_under_limit() {
        let payload = br#"{"ok":true}"#;
        let compressed = gzip_compress(payload);

        let out = decompress_body_with_limit("gzip", &compressed, 1024)
            .unwrap()
            .unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn decompress_body_with_limit_allows_exactly_limit_bytes() {
        let payload = vec![7u8; 64 * 1024];
        let compressed = gzip_compress(&payload);

        let out = decompress_body_with_limit("gzip", &compressed, 64 * 1024)
            .unwrap()
            .unwrap();
        assert_eq!(out.len(), 64 * 1024);
        assert_eq!(out, payload);
    }

    #[test]
    fn decompress_body_with_limit_aborts_gzip_bomb_mid_stream() {
        // 4 MiB of pseudo-random data (roughly 1:1 compression) gzipped then truncated to 2 MiB: the
        // stream ends abruptly after producing about 2 MiB. A bounded read should report TooLarge when the
        // 1 MiB budget runs out, while an unbounded read runs to the truncated tail and reports
        // UnexpectedEof (Io); the two are distinguishable, so this test catches an "expand fully, then compare" regression.
        let payload = pseudo_random_bytes(4 * 1024 * 1024);
        let compressed = gzip_compress(&payload);
        assert!(compressed.len() > 2 * 1024 * 1024);
        let truncated = &compressed[..2 * 1024 * 1024];

        let result = decompress_body_with_limit("gzip", truncated, 1024 * 1024);
        assert!(
            matches!(result, Err(DecompressError::TooLarge { .. })),
            "should stop when the budget runs out (TooLarge), not error only at the end of the stream: {:?}",
            result.as_ref().map(|o| o.as_ref().map(Vec::len))
        );
    }

    #[test]
    fn decompress_body_with_limit_rejects_zstd_bomb() {
        // High-ratio payload: 8 MiB of zeros compresses to a few KiB with zstd, so a full expansion must exceed the cap
        let payload = vec![0u8; 8 * 1024 * 1024];
        let compressed = zstd::stream::encode_all(std::io::Cursor::new(&payload[..]), 0).unwrap();
        assert!(compressed.len() < 1024 * 1024);

        let result = decompress_body_with_limit("zstd", &compressed, 1024 * 1024);
        assert!(
            matches!(result, Err(DecompressError::TooLarge { .. })),
            "a zstd compression bomb should stop when the budget runs out: {:?}",
            result.as_ref().map(|o| o.as_ref().map(Vec::len))
        );
    }

    #[test]
    fn decompress_body_with_limit_rejects_brotli_bomb() {
        let payload = vec![0u8; 8 * 1024 * 1024];
        let mut compressed = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
            std::io::Write::write_all(&mut writer, &payload).unwrap();
        }
        assert!(compressed.len() < 1024 * 1024);

        let result = decompress_body_with_limit("br", &compressed, 1024 * 1024);
        assert!(
            matches!(result, Err(DecompressError::TooLarge { .. })),
            "a brotli compression bomb should stop when the budget runs out: {:?}",
            result.as_ref().map(|o| o.as_ref().map(Vec::len))
        );
    }

    #[test]
    fn decompress_body_with_limit_bounds_intermediate_stage_of_stacked_encodings() {
        // Stacked encoding gzip, zstd: zstd first yields the (small) gzip stream, which then expands to 8 MiB.
        // The intermediate result is budgeted too; the guard cannot sit only on the last stage.
        let payload = vec![0u8; 8 * 1024 * 1024];
        let gzipped = gzip_compress(&payload);
        let stacked = zstd::stream::encode_all(std::io::Cursor::new(&gzipped[..]), 0).unwrap();

        let result = decompress_body_with_limit("gzip, zstd", &stacked, 1024 * 1024);
        assert!(
            matches!(result, Err(DecompressError::TooLarge { .. })),
            "intermediate output of a stacked encoding must be budgeted too: {:?}",
            result.as_ref().map(|o| o.as_ref().map(Vec::len))
        );
    }

    #[test]
    fn is_supported_content_encoding_matches_decompressable() {
        for enc in [
            "gzip",
            "x-gzip",
            "deflate",
            "br",
            "zstd",
            "zst",
            "gzip, zstd",
        ] {
            assert!(
                is_supported_content_encoding(enc),
                "{enc} should be supported"
            );
        }
        for enc in ["identity", "snappy", "compress", "", "gzip, snappy"] {
            assert!(
                !is_supported_content_encoding(enc),
                "{enc} should not be supported"
            );
        }
    }

    #[test]
    fn get_content_encoding_combines_repeated_headers() {
        // Repeated content-encoding headers equal a comma join, so merge them with get_all
        let mut headers = HeaderMap::new();
        headers.append("content-encoding", HeaderValue::from_static("gzip"));
        headers.append("content-encoding", HeaderValue::from_static("zstd"));
        assert_eq!(
            get_content_encoding(&headers).as_deref(),
            Some("gzip, zstd")
        );
    }

    #[test]
    fn get_content_encoding_ignores_identity_only() {
        let mut headers = HeaderMap::new();
        headers.append("content-encoding", HeaderValue::from_static("identity"));
        assert_eq!(get_content_encoding(&headers), None);
    }
}
