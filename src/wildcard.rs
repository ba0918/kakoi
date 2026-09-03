//! The shell wildcards a policy may use in `scan.names`, `scan.exclude`, `scan.prune`, and
//! `env.unset`: `*` for any run of bytes and `?` for one byte (specification sections 5.3
//! and 6.3). Nothing else is special: a glob crate would also accept `[...]`.

/// Whether `text` matches `pattern`, byte by byte.
pub fn matches(pattern: &str, text: &[u8]) -> bool {
    matches_bytes(pattern.as_bytes(), text)
}

fn matches_bytes(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|skip| matches_bytes(rest, &text[skip..])),
        Some((b'?', rest)) => !text.is_empty() && matches_bytes(rest, &text[1..]),
        Some((byte, rest)) => text.first() == Some(byte) && matches_bytes(rest, &text[1..]),
    }
}
