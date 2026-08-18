use std::fmt::{self, Debug, Display, Formatter};

/// A Git object identifier.
///
/// SHA-1 only. A SHA-256 repository is rejected at the adapter boundary rather than
/// represented here: widening this type to a length-tagged buffer would cost every
/// commit in a 100 000-commit history twelve bytes and the `Copy` impl the graph
/// layout relies on, to serve a repository format gitr has never been pointed at.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId([u8; 20]);

/// Number of hexadecimal characters in a full [`ObjectId`].
pub const HEX_LEN: usize = 40;

impl ObjectId {
    pub const fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Renders the first `len` hexadecimal characters, clamped to [`HEX_LEN`].
    pub fn to_hex_prefix(self, len: usize) -> String {
        let len = len.min(HEX_LEN);
        let mut out = String::with_capacity(len);
        for byte in self.0 {
            if out.len() >= len {
                break;
            }
            out.push(nibble_to_hex(byte >> 4));
            if out.len() < len {
                out.push(nibble_to_hex(byte & 0x0f));
            }
        }
        out
    }
}

const fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'a' + nibble - 10) as char,
    }
}

const fn hex_to_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Rejects anything that is not exactly forty hexadecimal characters.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseObjectIdError {
    #[error("expected {HEX_LEN} hexadecimal characters, got {0}")]
    WrongLength(usize),
    #[error("{0:?} is not a hexadecimal character")]
    NotHexadecimal(char),
}

impl std::str::FromStr for ObjectId {
    type Err = ParseObjectIdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bytes = text.as_bytes();
        if bytes.len() != HEX_LEN {
            return Err(ParseObjectIdError::WrongLength(bytes.len()));
        }
        let mut out = [0u8; 20];
        for (index, pair) in bytes.chunks_exact(2).enumerate() {
            let high = hex_to_nibble(pair[0])
                .ok_or(ParseObjectIdError::NotHexadecimal(pair[0] as char))?;
            let low = hex_to_nibble(pair[1])
                .ok_or(ParseObjectIdError::NotHexadecimal(pair[1] as char))?;
            out[index] = (high << 4) | low;
        }
        Ok(Self(out))
    }
}

impl Display for ObjectId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex_prefix(HEX_LEN))
    }
}

impl Debug for ObjectId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "ObjectId({})", self.to_hex_prefix(HEX_LEN))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const SAMPLE: &str = "d7c8a3072d02017729946165fa25f84fe35476e7";

    #[test]
    fn parses_and_renders_the_same_hex() {
        let id = ObjectId::from_str(SAMPLE).unwrap();
        assert_eq!(id.to_string(), SAMPLE);
    }

    #[test]
    fn accepts_uppercase_hex() {
        let id = ObjectId::from_str(&SAMPLE.to_uppercase()).unwrap();
        assert_eq!(id.to_string(), SAMPLE);
    }

    #[test]
    fn rejects_a_short_string() {
        assert_eq!(
            ObjectId::from_str("d7c8a30"),
            Err(ParseObjectIdError::WrongLength(7))
        );
    }

    #[test]
    fn rejects_a_non_hexadecimal_character() {
        let text = format!("z{}", &SAMPLE[1..]);
        assert_eq!(
            ObjectId::from_str(&text),
            Err(ParseObjectIdError::NotHexadecimal('z'))
        );
    }

    #[test]
    fn truncates_the_prefix_to_the_requested_length() {
        let id = ObjectId::from_str(SAMPLE).unwrap();
        assert_eq!(id.to_hex_prefix(7), "d7c8a30");
        assert_eq!(id.to_hex_prefix(0), "");
    }

    #[test]
    fn clamps_a_prefix_longer_than_the_identifier() {
        let id = ObjectId::from_str(SAMPLE).unwrap();
        assert_eq!(id.to_hex_prefix(100), SAMPLE);
    }
}
