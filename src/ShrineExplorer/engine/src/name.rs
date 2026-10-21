//! The track name: a short label chosen in the setup, carried in the recipe,
//! shown at the head of the page and used as the exported WAV's title.
//!
//! Fixed-size and `Copy` so the whole configuration stays `Copy` and the recipe
//! stays a plain array of `u32` words. Only printable ASCII is kept, so the
//! name survives any encoding the browser or a tag reader may apply.

/// The longest name, in bytes (8 recipe words).
pub const CAP: usize = 32;

/// A short, user-chosen track name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Name {
    bytes: [u8; CAP],
    len: u8,
}

impl Name {
    pub const EMPTY: Name = Name { bytes: [0; CAP], len: 0 };

    /// Build from a string, keeping printable ASCII and truncating to [`CAP`].
    pub fn from_str(text: &str) -> Name {
        let mut bytes = [0u8; CAP];
        let mut len = 0usize;
        for ch in text.chars() {
            if len >= CAP {
                break;
            }
            bytes[len] = if ch.is_ascii_graphic() || ch == ' ' { ch as u8 } else { b'?' };
            len += 1;
        }
        Name { bytes, len: len as u8 }
    }

    /// The name as text. Never fails: only printable ASCII is stored.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The eight recipe words holding the name.
    pub fn words(&self) -> [u32; CAP / 4] {
        let mut out = [0u32; CAP / 4];
        for (i, w) in out.iter_mut().enumerate() {
            let b = i * 4;
            *w = u32::from_le_bytes([self.bytes[b], self.bytes[b + 1], self.bytes[b + 2], self.bytes[b + 3]]);
        }
        out
    }

    /// Rebuild from recipe words. Stops at the first NUL and drops anything
    /// that is not printable ASCII, so corrupt input cannot inject control
    /// bytes into a page or a tag.
    pub fn from_words(words: &[u32]) -> Name {
        let mut raw = [0u8; CAP];
        for (i, w) in words.iter().take(CAP / 4).enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        let mut bytes = [0u8; CAP];
        let mut len = 0usize;
        for &c in raw.iter() {
            if c == 0 {
                break;
            }
            if (0x20..=0x7e).contains(&c) {
                bytes[len] = c;
                len += 1;
            }
        }
        Name { bytes, len: len as u8 }
    }
}

impl Default for Name {
    fn default() -> Self {
        Name::EMPTY
    }
}

impl std::fmt::Debug for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Name({:?})", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_round_trips_through_words() {
        for text in ["", "Moss Garden", "Shrine Explorer", "a", "The Long Tide at Dusk"] {
            let name = Name::from_str(text);
            assert_eq!(name.as_str(), text);
            assert_eq!(Name::from_words(&name.words()), name);
        }
    }

    #[test]
    fn overlong_names_are_truncated_to_capacity() {
        let long = "x".repeat(CAP * 2);
        let name = Name::from_str(&long);
        assert_eq!(name.as_str().len(), CAP);
    }

    #[test]
    fn non_printable_input_is_replaced_or_dropped() {
        assert_eq!(Name::from_str("a\tb").as_str(), "a?b");
        // A word array full of junk stops at the first NUL and drops controls.
        let words = [u32::MAX; CAP / 4];
        let name = Name::from_words(&words);
        assert!(name.as_str().chars().all(|c| c.is_ascii_graphic() || c == ' '));
        assert_eq!(Name::from_words(&[0; CAP / 4]), Name::EMPTY);
    }
}
