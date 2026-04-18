//! Prompt-injection defense envelope for Spacedrive-returned bytes.
//!
//! ADR: docs/design-docs/spacedrive-tool-response-envelope.md
//!
//! Every tool that returns Spacedrive-originated content to the LLM MUST
//! wrap it via `wrap_spacedrive_response`. The envelope provides:
//! - provenance tag `[SPACEDRIVE:<library_id>:<wire_method>]`
//! - untrusted-content fences `<<<UNTRUSTED_SPACEDRIVE_CONTENT>>>` ... `<<<END_...>>>`
//! - byte-cap truncation
//! - control-character stripping

const FENCE_OPEN: &str = "<<<UNTRUSTED_SPACEDRIVE_CONTENT>>>";
const FENCE_CLOSE: &str = "<<<END_UNTRUSTED_SPACEDRIVE_CONTENT>>>";

pub const DEFAULT_CAP: usize = 10 * 1024 * 1024;
pub const CAP_LIST_FILES: usize = 64 * 1024;
pub const CAP_READ_FILE: usize = 1024 * 1024;
pub const CAP_CONTEXT_LOOKUP: usize = 16 * 1024;

/// Wrap raw bytes from Spacedrive into a safe envelope for LLM consumption.
///
/// Arguments:
/// - `library_id`: the paired library ID, stringified. If unpaired, pass `"none"`.
/// - `wire_method`: the Spacedrive method the bytes came from (e.g.
///   `query:media_listing`).
/// - `raw`: the bytes returned by Spacedrive. Typically JSON but may be any
///   content; the envelope is content-agnostic.
/// - `byte_cap`: maximum bytes to include. Overflow produces a truncation
///   marker after the closing fence.
pub fn wrap_spacedrive_response(
    library_id: &str,
    wire_method: &str,
    raw: &[u8],
    byte_cap: usize,
) -> String {
    let (truncated_bytes, total_size) = truncate(raw, byte_cap);
    let sanitized = strip_control_chars(truncated_bytes);

    let mut out = String::with_capacity(sanitized.len() + 256);
    out.push_str(&format!("[SPACEDRIVE:{library_id}:{wire_method}]\n"));
    out.push_str(FENCE_OPEN);
    out.push('\n');
    out.push_str(&sanitized);
    if !sanitized.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(FENCE_CLOSE);
    if total_size > byte_cap {
        out.push_str(&format!(
            "\n[...truncated, original size {total_size} bytes, cap {byte_cap}]"
        ));
    }
    out
}

fn truncate(raw: &[u8], byte_cap: usize) -> (&[u8], usize) {
    let total = raw.len();
    let cut = if total > byte_cap { byte_cap } else { total };
    (&raw[..cut], total)
}

fn strip_control_chars(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x00' => {}
            '\x1b' => {
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\x07' || next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            '\t' | '\n' | '\r' => out.push(ch),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_small_payload_cleanly() {
        let raw = br#"{"files": []}"#;
        let out = wrap_spacedrive_response("lib", "query:media_listing", raw, 1024);
        assert!(out.starts_with("[SPACEDRIVE:lib:query:media_listing]"));
        assert!(out.contains(FENCE_OPEN));
        assert!(out.contains(FENCE_CLOSE));
        assert!(out.contains(r#"{"files": []}"#));
        assert!(!out.contains("truncated"));
    }

    #[test]
    fn truncates_oversized_payload() {
        let raw = vec![b'x'; 2048];
        let out = wrap_spacedrive_response("lib", "query:x", &raw, 1024);
        assert!(out.contains("[...truncated, original size 2048 bytes, cap 1024]"));
    }

    #[test]
    fn strips_ansi_escapes() {
        let raw = b"plain\x1b[31mRED\x1b[0mmore";
        let out = wrap_spacedrive_response("lib", "query:x", raw, 1024);
        assert!(!out.contains('\x1b'), "ANSI escape leaked into envelope");
        assert!(out.contains("plain"));
    }

    #[test]
    fn strips_nul_bytes() {
        let raw = b"hello\x00world";
        let out = wrap_spacedrive_response("lib", "query:x", raw, 1024);
        assert!(!out.contains('\x00'));
        assert!(out.contains("helloworld"));
    }

    #[test]
    fn fence_survives_json_roundtrip() {
        let raw = br#"{"name": "report.pdf"}"#;
        let out = wrap_spacedrive_response("lib", "query:x", raw, 1024);
        let json = serde_json::to_string(&out).unwrap();
        let back: String = serde_json::from_str(&json).unwrap();
        assert!(back.contains(FENCE_OPEN));
        assert!(back.contains(FENCE_CLOSE));
    }
}
