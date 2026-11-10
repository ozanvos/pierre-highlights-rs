# pierre-highlights-rs

Unofficial Rust port of [Pierre Highlights](https://github.com/pierrecomputer/pierre/tree/45e4f46a07060930cfa22fdc2b90e8e4752fc789/packages/highlights). Tracks Highlights 0.4.0. Supports 152 languages plus plain text, with 73 token kinds and no runtime dependencies. Requires Rust 1.88+.

## Use

Add the crate to your project:

```toml
[dependencies]
pierre-highlights-rs = "0.2.0"
```

```rust
use pierre_highlights_rs::{Highlighter, Language};

let mut highlighter = Highlighter::new();
let source = "fn main() { println!(\"hello\"); }";
let language = Language::from_name("rust").unwrap();
for token in highlighter.highlight(language, source).iter() {
    println!("{}: {}", token.kind.name(), &source[token.range]);
}
```

Reuse one `Highlighter` per worker. Results borrow its buffer and use UTF-8 byte ranges. `Language::ALL` lists languages; `Language::from_name()` accepts aliases and returns `None` for unsupported names. The buffer starts at 192 KiB and retains capacity after growth. Inputs over 64 MiB panic.

The API tokenizes a whole buffer. Themes, rendering, incremental editing, and fallback selection are left to the caller.

## Develop

```sh
just check # Check formatting, run Clippy and tests
just test  # Test native SIMD and scalar implementations
just fmt   # Format Rust source
```

The recipes use Cargo; `just` is optional. Tests check 50,151 saved reference cases from Pierre Highlights 0.4.0, plus language aliases, UTF-8 ranges, buffer reuse, and SIMD masks. The `scalar` feature selects the portable implementation.

## Upgrading from 0.1

Language IDs now follow the Highlights 0.4.0 table and have changed. Persist language names, not numeric IDs; resolve names with `Language::from_name()`. The 73 token kinds and the Rust highlighting API are unchanged. New languages include PromQL, Lean, ABAP, Apex, and many others; see `Language::ALL` for the complete catalog.

The crate remains a whole-buffer tokenizer. Upstream ANSI/HTML rendering, themes, streaming, and live-editing APIs are not exposed.

## Updating the upstream port

Generated source, static memory, aliases, and reference fixtures are checked in. Normal builds only need Rust. Maintenance additionally needs Python 3, Bun, npm, and a Pierre checkout:

```sh
git clone https://github.com/pierrecomputer/pierre.git /tmp/pierre
git -C /tmp/pierre checkout 45e4f46a07060930cfa22fdc2b90e8e4752fc789
npm ci --prefix scripts --ignore-scripts
bun scripts/sync.ts /tmp/pierre/packages/highlights
just check
```

The sync script verifies the package tree against `scripts/upstream.json`, uses upstream's WAT expansion and validation, then translates to native Rust. It retains the existing SIMD implementation and raw UTF-8 record adaptation (bypassing upstream's UTF-16 line conversion). Fixtures are evaluated independently with upstream Wasm, preserving the existing inputs and adding all-language samples, truncations, Unicode, malformed inputs, and deterministic fuzz cases. Unsupported translation instructions fail rather than silently falling back.

When updating the pin, review changes to the memory layout, language/token IDs, Unicode identifier rules, and emitter adaptation as well as lexer behavior. Update the version/provenance comments and verify both native SIMD and scalar paths. Generated output is not hand-edited.

## License

[Apache-2.0](LICENSE). See [NOTICE.md](NOTICE.md) for attribution.
