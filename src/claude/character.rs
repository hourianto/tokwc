//! Packed BMP properties used by the normalization and boundary scans.
use std::sync::LazyLock;

use super::compression::decode_array;
pub(super) use super::properties::Character;

pub(super) struct CharacterTable(Box<[u8; 65536 * 2]>);

pub(super) fn table() -> &'static CharacterTable {
    static TABLE: LazyLock<CharacterTable> =
        LazyLock::new(|| CharacterTable(decode_array(include_bytes!("data/character.bin.zst"))));
    &TABLE
}

impl Character {
    #[inline]
    pub(super) fn get(ch: char) -> Self {
        table().get(ch)
    }
}

impl CharacterTable {
    #[inline]
    pub(super) fn get(&self, ch: char) -> Character {
        let cp = ch as usize;
        if cp < 65536 {
            Character(u16::from_le_bytes([self.0[cp * 2], self.0[cp * 2 + 1]]))
        } else {
            Character::compute(ch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_table_matches_unicode_rules() {
        for cp in 0..65536 {
            if let Some(ch) = char::from_u32(cp) {
                assert_eq!(Character::get(ch).0, Character::compute(ch).0, "U+{cp:04X}");
            }
        }
    }
}
