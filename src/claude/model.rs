use std::sync::{LazyLock, OnceLock};

use super::ClaudeVersion;
use super::compression::{decode, decode_array};

static ASCII_WORDS: LazyLock<Box<[u8; 128 + 128 * 128]>> =
    LazyLock::new(|| decode_array(include_bytes!("data/ascii_words.bin.zst")));

/// Costs include the word boundaries and any title-case marker.
pub(super) fn short_word_cost(body: &str) -> Option<usize> {
    let key = match *body.as_bytes() {
        [a] if a.is_ascii_alphabetic() => a as usize,
        [a, b] if a.is_ascii_alphabetic() && b.is_ascii_alphabetic() => {
            128 + a as usize * 128 + b as usize
        }
        _ => return None,
    };
    Some(ASCII_WORDS[key] as usize)
}

static FLOOR_BITS: LazyLock<Box<[u8; 16_384]>> =
    LazyLock::new(|| decode_array(include_bytes!("data/floor_v4_7.bin.zst")));
// The first bitmap contains one-token BMP scalars; the second contains two-byte UTF-8 prefixes.
const UNIT_BITS: usize = 0;
const PREFIX_BITS: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameTail {
    Keep,
    StripTwoNewlines,
    StripWhitespace,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FamilyConfig {
    pub(crate) message_overhead: usize,
    pub(crate) frame_bow: bool,
    pub(crate) frame_tail: FrameTail,
}

impl FamilyConfig {
    pub(crate) const fn for_version(version: ClaudeVersion) -> Self {
        match version {
            ClaudeVersion::V4_7 => Self {
                message_overhead: 11,
                frame_bow: true,
                frame_tail: FrameTail::Keep,
            },
            ClaudeVersion::V4_8 => Self {
                message_overhead: 6,
                frame_bow: false,
                frame_tail: FrameTail::StripTwoNewlines,
            },
            // Opus 5 shares the vocabulary, but absorbs raw trailing ASCII whitespace.
            ClaudeVersion::V5 => Self {
                message_overhead: 6,
                frame_bow: false,
                frame_tail: FrameTail::StripWhitespace,
            },
            ClaudeVersion::FableV5_1 | ClaudeVersion::V5_5 => Self {
                message_overhead: 8,
                frame_bow: false,
                frame_tail: FrameTail::StripWhitespace,
            },
        }
    }
}

pub(crate) struct Vocabulary {
    dfa: Vec<u32>,
}

/// Only the root and validated table transitions can construct a row offset.
#[derive(Clone, Copy)]
pub(super) struct State(usize);

impl State {
    pub(super) const ROOT: Self = Self(0);
}

pub(crate) fn vocabulary() -> &'static Vocabulary {
    static VOCABULARY: OnceLock<Vocabulary> = OnceLock::new();
    VOCABULARY.get_or_init(Vocabulary::load)
}

impl Vocabulary {
    fn load() -> Self {
        // Blocks are deduplicated on disk, then expanded once to keep the hot loop
        // to a single table lookup per byte. All integers are little-endian.
        // Source: ctok pieces_v4_7.json at ad78ea15a1febf983b379475b20f5a2b0d2ebe76.
        let data = decode(include_bytes!("data/count_dfa_v4_7.bin.zst"));
        assert_eq!(&data[..8], b"TOKDFA01");
        let read_u32 = |bytes: &[u8]| u32::from_le_bytes(bytes.try_into().unwrap());
        let states = read_u32(&data[8..12]) as usize;
        assert!(states > 0);
        let block_count = read_u32(&data[12..16]) as usize;
        let split = 16 + states * 16 * 2;
        assert_eq!(data.len(), split + block_count * 16 * 4);
        // Validate each distinct block once, then copy whole rows of transitions.
        let blocks: Vec<[u32; 16]> = data[split..]
            .as_chunks::<64>()
            .0
            .iter()
            .map(|block| {
                std::array::from_fn(|index| {
                    let edge = read_u32(&block[index * 4..index * 4 + 4]);
                    assert!(((edge >> 8) as usize) < states);
                    edge
                })
            })
            .collect();
        let mut dfa = Vec::with_capacity(states * 256);
        for index in data[16..split].as_chunks::<2>().0 {
            let block = u16::from_le_bytes(*index) as usize;
            dfa.extend_from_slice(&blocks[block]);
        }
        Self { dfa }
    }

    #[inline]
    pub(super) fn step(&self, state: State, byte: u8) -> (State, i8) {
        // SAFETY: load() checks every destination row. Callers begin at row zero and only
        // use a returned edge's upper bits as the next row, so this index is valid.
        let edge = unsafe { *self.dfa.get_unchecked(state.0 | byte as usize) };
        (
            State((edge & !255) as usize),
            i8::from_le_bytes([edge.to_le_bytes()[0]]),
        )
    }
}

pub(super) fn scalar_only(ch: char) -> bool {
    static MULTI_SCALAR: LazyLock<Box<[u8; 8192]>> =
        LazyLock::new(|| decode_array(include_bytes!("data/multi_scalar.bin.zst")));
    let cp = ch as usize;
    cp >= 0x10000 || MULTI_SCALAR[cp / 8] & (1 << (cp % 8)) == 0
}

pub(super) fn char_cost(ch: char) -> usize {
    let cp = ch as usize;
    if cp < 0x80 || (cp < 0x10000 && bit_is_set(UNIT_BITS, cp)) {
        return 1;
    }
    if cp < 0x800 {
        return 2;
    }
    // Only the leading bytes influence fallback cost. Derive them directly from
    // the scalar instead of re-encoding UTF-8 into a temporary buffer.
    let (length, prefix) = if cp < 0x10000 {
        (3, ((0xe0 | (cp >> 12)) << 8) | (0x80 | ((cp >> 6) & 63)))
    } else {
        if matches!(cp >> 6, 0x7d1 | 0x7d2 | 0x7d3 | 0x7d8) {
            return 2;
        }
        (4, ((0xf0 | (cp >> 18)) << 8) | (0x80 | ((cp >> 12) & 63)))
    };
    length - usize::from(bit_is_set(PREFIX_BITS, prefix))
}

#[inline]
fn bit_is_set(offset: usize, value: usize) -> bool {
    FLOOR_BITS[offset + value / 8] & (1 << (value % 8)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_floor_matches_counter() {
        let vocabulary = vocabulary();
        for cp in 0..=0x0010_ffff {
            if let Some(ch) = char::from_u32(cp) {
                let mut buffer = [0; 4];
                let mut state = State::ROOT;
                let mut expected = 0isize;
                for byte in ch.encode_utf8(&mut buffer).bytes() {
                    let (next, delta) = vocabulary.step(state, byte);
                    state = next;
                    expected += isize::from(delta);
                }
                assert_eq!(char_cost(ch), usize::try_from(expected).unwrap(), "{ch:?}");
            }
        }
    }
}
