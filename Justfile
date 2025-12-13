# Cross-platform CI tasks
ci:
    just test
    just nostd
    just clippy
    cargo fmt --check --all

# Run tests using cargo-nextest
test *args:
    cargo nextest run {{args}}

# Check no_std compatibility
nostd:
    cargo check --no-default-features -p pk2

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Feature matrix testing
features:
    @echo "Testing no default features..."
    cargo check --no-default-features -p pk2
    @echo "Testing std feature only..."
    cargo check --no-default-features --features std -p pk2
    @echo "Testing euc-kr feature only..."
    cargo check --no-default-features --features euc-kr -p pk2
    @echo "Testing all features..."
    cargo check --all-features --workspace
    @echo "All feature combinations passed!"

# Run tests with all features
test-all-features:
    cargo nextest run --workspace --all-features

# Build documentation
docs:
    cargo doc --workspace --no-deps --document-private-items

# Clean build artifacts
clean:
    cargo clean
