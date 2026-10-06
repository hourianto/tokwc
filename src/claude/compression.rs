use std::io::Read;

/// Decode a single Zstandard frame embedded in the binary.
pub(super) fn decode(data: &[u8]) -> Vec<u8> {
    let mut decoder =
        ruzstd::decoding::StreamingDecoder::new(data).expect("invalid embedded Claude table frame");
    let mut bytes = Vec::new();
    decoder
        .read_to_end(&mut bytes)
        .expect("invalid embedded Claude table data");
    bytes
}

pub(super) fn decode_array<const N: usize>(data: &[u8]) -> Box<[u8; N]> {
    decode(data)
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| panic!("invalid embedded Claude table length"))
}
