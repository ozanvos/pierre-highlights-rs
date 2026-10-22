//! Native Rust port of Pierre Highlights 0.2.0. No Wasm runtime.
//! Reuse a [`Highlighter`] to retain allocation capacity. Tokens use UTF-8 byte
//! offsets and Pierre token kinds; rendering and theme mapping are separate.
mod catalog;
mod generated;
mod memory;
mod simd;
mod unicode;
pub use catalog::{Language, TokenKind};
use generated::Engine;

/// A contiguous, lossless source span. Whitespace follows Pierre's token rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub range: std::ops::Range<usize>,
    pub kind: TokenKind,
}

/// Reusable, thread-owned lexer state. Use one per worker.
pub struct Highlighter {
    engine: Engine,
}
impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}
impl Highlighter {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine.raw_records = true;
        Self { engine }
    }
    /// Highlight valid UTF-8. Panics for inputs above 64 MiB.
    /// Returned records borrow the reusable arena until the next call.
    pub fn highlight(&mut self, language: Language, source: &str) -> Records<'_> {
        assert!(source.len() <= 64 * 1024 * 1024, "input exceeds 64 MiB");
        let required = 65536 + source.len() + 96;
        if self.engine.memory.len() < required {
            self.engine
                .memory
                .resize(required.next_multiple_of(65536), 0);
        }
        let end = 65536 + source.len();
        self.engine.memory[65536..end].copy_from_slice(source.as_bytes());
        self.engine.memory[end..end + 32].fill(0);
        self.engine.memory[0] = language.id();
        self.engine.memory[1] = 3;
        self.engine.i32_store(2, source.len() as u32);
        self.engine.w_highlight();
        let start = self.engine.i32_load(6) as usize;
        let len = self.engine.i32_load(10) as usize;
        Records {
            bytes: &self.engine.memory[start..start + len],
        }
    }
}

/// Packed, little-endian `(end_byte: u32, token_kind: u32)` pairs.
#[derive(Clone, Copy)]
pub struct Records<'a> {
    bytes: &'a [u8],
}
impl<'a> Records<'a> {
    pub fn len(self) -> usize {
        self.bytes.len() / 8
    }
    pub fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }
    pub fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub fn iter(self) -> impl ExactSizeIterator<Item = Token> + 'a {
        let mut start = 0;
        self.bytes.as_chunks::<8>().0.iter().map(move |r| {
            let end = u32::from_le_bytes(r[..4].try_into().unwrap()) as usize;
            let kind = TokenKind::from_id(u32::from_le_bytes(r[4..].try_into().unwrap()) as u8)
                .expect("valid lexer token");
            let token = Token {
                range: start..end,
                kind,
            };
            start = end;
            token
        })
    }
}
