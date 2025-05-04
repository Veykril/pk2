ci:
    just test
    just nostd
    just clippy
    cargo fmt --check --all

nostd:
    cargo check --no-default-features -p pk2

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test *args:
    cargo nextest run {{args}} < /dev/null
