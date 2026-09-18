//! 4096-byte XOR pads derived from a u64 seed via MT19937-64.

use super::mt19937_64::Mt19937_64;

pub const PAD_SIZE: usize = 4096;

pub type Pad = [u8; PAD_SIZE];

/// Draw 512 u64s and lay them out as bytes. The initial (pre-handshake) pad
/// uses native little-endian layout; the session pad byte-swaps each word.
pub fn generate(seed: u64, big_endian: bool) -> Box<Pad> {
    let mut rng = Mt19937_64::new(seed);
    let mut pad = Box::new([0u8; PAD_SIZE]);
    for chunk in pad.as_chunks_mut::<8>().0 {
        let v = rng.next_u64();
        *chunk = if big_endian { v.to_be_bytes() } else { v.to_le_bytes() };
    }
    pad
}

pub fn initial(seed: u64) -> Box<Pad> {
    generate(seed, false)
}

pub fn session(key: u64) -> Box<Pad> {
    generate(key, true)
}

pub fn apply(pad: &Pad, data: &mut [u8]) {
    for (i, b) in data.iter_mut().enumerate() {
        *b ^= pad[i % PAD_SIZE];
    }
}

pub fn xored(pad: &Pad, data: &[u8]) -> Vec<u8> {
    let mut v = data.to_vec();
    apply(pad, &mut v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endianness_layouts() {
        let first = Mt19937_64::new(42).next_u64();
        assert_eq!(&initial(42)[..8], &first.to_le_bytes());
        assert_eq!(&session(42)[..8], &first.to_be_bytes());
    }

    #[test]
    fn apply_is_involution_and_wraps() {
        let pad = initial(7);
        let data: Vec<u8> = (0..PAD_SIZE + 100).map(|i| i as u8).collect();
        let mut x = data.clone();
        apply(&pad, &mut x);
        assert_eq!(x[PAD_SIZE] ^ pad[0], data[PAD_SIZE]);
        apply(&pad, &mut x);
        assert_eq!(x, data);
    }
}
