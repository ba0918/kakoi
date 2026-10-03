//! The shell wildcards a policy may use in `scan.names`, `scan.exclude`, `scan.prune`, and
//! `env.unset`: `*` for any run of bytes and `?` for one byte (specification sections 5.3
//! and 6.3). Nothing else is special: a glob crate would also accept `[...]`.

/// Whether `text` matches `pattern`, byte by byte.
pub fn matches(pattern: &str, text: &[u8]) -> bool {
    matches_bytes(pattern.as_bytes(), text)
}

/// Linear in the lengths: a `*` is given the shortest run first, and a later mismatch
/// backtracks to the most recent `*` alone. Trying every split of every `*` instead would
/// take time exponential in the number of stars, and the text is a file name the
/// isolation can write.
fn matches_bytes(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some(b'*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(b'?') => {
                p += 1;
                t += 1;
            }
            Some(byte) if *byte == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|byte| *byte == b'*')
}
