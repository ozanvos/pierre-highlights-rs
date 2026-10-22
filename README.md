# pierre-highlights-rs

Unofficial Rust port of [Pierre Highlights](https://github.com/pierrecomputer/pierre/tree/2d2498e77f31273811d96c3208d5605f449d2262/packages/highlights). Supports 80 languages plus plain text, with 73 token kinds and no runtime dependencies. Requires Rust 1.88+.

## Use

The crate is unpublished. Add it from a sibling checkout:

```toml
[dependencies]
pierre-highlights-rs = { path = "../pierre-highlights-rs" }
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

The recipes use Cargo; `just` is optional. Maintain the Rust source directly. Tests check 22,148 saved reference cases from Pierre Highlights 0.2.0, plus language aliases, UTF-8 ranges, buffer reuse, and SIMD masks. The `scalar` feature selects the portable implementation.

## License

[Apache-2.0](LICENSE). See [NOTICE.md](NOTICE.md) for attribution.
