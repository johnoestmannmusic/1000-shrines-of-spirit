//! A minimal, dependency-free PNG writer (8-bit RGBA) with a fixed-Huffman
//! DEFLATE encoder.
//!
//! This lives in the host only. It is how we look at the engine while building
//! it, and it keeps the "no binary assets / no dependencies" promise: previews
//! are generated from the same Rust that generates the art. The DEFLATE encoder
//! matters here — a 1600x1600 cover is 10 MB uncompressed but ~1 MB compressed.

/// Encode an RGBA8 image as a PNG byte stream.
pub fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), (width as usize) * (height as usize) * 4);

    // Raw scanlines, each prefixed with filter type 0 (None).
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * stride..(y + 1) * stride]);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, adaptive, no interlace
    chunk(&mut out, b"IHDR", &ihdr);

    chunk(&mut out, b"IDAT", &zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// zlib wrapper around one DEFLATE fixed-Huffman block.
fn zlib(raw: &[u8]) -> Vec<u8> {
    let mut bits = BitWriter::new();
    bits.write_bits(1, 1); // BFINAL
    bits.write_bits(1, 2); // BTYPE = 01 (fixed Huffman)
    compress_fixed(raw, &mut bits);
    bits.write_code(0, 7); // end-of-block symbol 256
    bits.flush();

    let mut out = vec![0x78, 0x01];
    out.extend_from_slice(bits.bytes());
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
const LENGTH_EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Greedy LZ77 with a one-slot-per-key hash chain, emitting fixed-Huffman
/// symbols. Flat, upscaled pixel art compresses very well.
fn compress_fixed(data: &[u8], bits: &mut BitWriter) {
    const HASH_BITS: usize = 15;
    let mask = (1usize << HASH_BITS) - 1;
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let n = data.len();
    let mut i = 0;
    while i < n {
        if i + 3 <= n {
            let key = hash3(data, i) & mask;
            let candidate = head[key];
            head[key] = i;
            if candidate != usize::MAX && i - candidate <= 32768 {
                let len = match_len(data, candidate, i);
                if len >= 3 {
                    emit_match(bits, len, i - candidate);
                    let end = (i + len).min(n.saturating_sub(2));
                    for j in (i + 1)..end {
                        head[hash3(data, j) & mask] = j;
                    }
                    i += len;
                    continue;
                }
            }
        }
        emit_symbol(bits, data[i] as u32);
        i += 1;
    }
}

fn hash3(data: &[u8], i: usize) -> usize {
    let a = data[i] as usize;
    let b = data[i + 1] as usize;
    let c = data[i + 2] as usize;
    (a << 5) ^ (b << 3) ^ c
}

fn match_len(data: &[u8], a: usize, b: usize) -> usize {
    let max = (data.len() - b).min(258);
    let mut len = 0;
    while len < max && data[a + len] == data[b + len] {
        len += 1;
    }
    len
}

fn emit_symbol(bits: &mut BitWriter, symbol: u32) {
    let (code, len) = fixed_literal(symbol);
    bits.write_code(code, len);
}

fn fixed_literal(symbol: u32) -> (u32, u32) {
    match symbol {
        0..=143 => (0x30 + symbol, 8),
        144..=255 => (0x190 + (symbol - 144), 9),
        256..=279 => (symbol - 256, 7),
        _ => (0xC0 + (symbol - 280), 8),
    }
}

fn emit_match(bits: &mut BitWriter, len: usize, dist: usize) {
    let code = LENGTH_BASE.iter().rposition(|base| len >= *base).unwrap_or(0);
    // rposition gives the last base <= len, which is the correct code.
    let lcode = code.min(28);
    emit_symbol(bits, 257 + lcode as u32);
    bits.write_bits((len - LENGTH_BASE[lcode]) as u32, LENGTH_EXTRA[lcode]);

    let dcode = DIST_BASE.iter().rposition(|base| dist >= *base).unwrap_or(0).min(29);
    bits.write_code(dcode as u32, 5); // fixed distance codes are 5-bit
    bits.write_bits((dist - DIST_BASE[dcode]) as u32, DIST_EXTRA[dcode]);
}

struct BitWriter {
    out: Vec<u8>,
    buffer: u32,
    count: u32,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter { out: Vec::new(), buffer: 0, count: 0 }
    }

    fn push_bit(&mut self, bit: u32) {
        self.buffer |= (bit & 1) << self.count;
        self.count += 1;
        if self.count == 8 {
            self.out.push(self.buffer as u8);
            self.buffer = 0;
            self.count = 0;
        }
    }

    /// Fields and extra bits are packed least-significant bit first.
    fn write_bits(&mut self, value: u32, count: u32) {
        for k in 0..count {
            self.push_bit((value >> k) & 1);
        }
    }

    /// Huffman codes are packed most-significant bit first.
    fn write_code(&mut self, code: u32, count: u32) {
        for k in (0..count).rev() {
            self.push_bit((code >> k) & 1);
        }
    }

    fn flush(&mut self) {
        if self.count > 0 {
            self.out.push(self.buffer as u8);
            self.buffer = 0;
            self.count = 0;
        }
    }

    fn bytes(&self) -> &[u8] {
        &self.out
    }
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(tag);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

struct Crc32 {
    value: u32,
}

impl Crc32 {
    fn new() -> Self {
        Crc32 { value: 0xffff_ffff }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.value ^= byte as u32;
            for _ in 0..8 {
                let mask = (self.value & 1).wrapping_neg();
                self.value = (self.value >> 1) ^ (0xedb8_8320 & mask);
            }
        }
    }

    fn finish(self) -> u32 {
        self.value ^ 0xffff_ffff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_spec_vector() {
        let mut crc = Crc32::new();
        crc.update(b"123456789");
        assert_eq!(crc.finish(), 0xcbf4_3926);
    }

    #[test]
    fn adler_matches_the_spec_vector() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn header_is_well_formed() {
        let png = encode_rgba(2, 1, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&png[0..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 12..], b"\x00\x00\x00\x00IEND\xae\x42\x60\x82");
    }

    /// The encoder is checked for validity by inflating a real PNG in Python
    /// (see the cover export); here we only lock in its behaviour.
    #[test]
    fn deflate_compresses_repetitive_data() {
        let raw = vec![7u8; 20_000];
        let z = zlib(&raw);
        assert!(z.len() < raw.len() / 8, "expected strong compression, got {} of {}", z.len(), raw.len());
    }

    #[test]
    fn deflate_is_deterministic() {
        let raw: Vec<u8> = (0..4000u32).map(|i| (i % 5) as u8).collect();
        assert_eq!(zlib(&raw), zlib(&raw));
    }
}
