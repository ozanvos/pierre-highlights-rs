default:
    @just --list

fmt:
    cargo fmt

test:
    cargo test --locked --release
    cargo test --locked --release --features scalar

check:
    cargo fmt -- --check
    cargo clippy --locked --all-targets -- -D warnings
    just test
