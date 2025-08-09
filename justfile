# Default recipe
default: build

# Build the project
build:
    cargo build

# Build in release mode
build-release:
    cargo build --release

# Build specific package
build-package package:
    cargo build -p {{package}}

# Build vsd with all features
build-vsd:
    cargo build -p vsd --all-features

# Build vsd for release
build-vsd-release:
    cargo build -p vsd --release --all-features

# Run tests
test:
    cargo test

# Run tests for specific package
test-package package:
    cargo test -p {{package}}

# Check code without building
check:
    cargo check

# Format code
fmt:
    cargo fmt

# Run clippy linter
clippy:
    cargo clippy -- -D warnings

# Clean build artifacts
clean:
    cargo clean

# Build documentation
docs:
    cargo doc --all-features --open

# Install vsd locally
install:
    cargo install --path vsd

# Cross-compile for multiple targets (requires cross)
cross-build:
    cross build --target x86_64-unknown-linux-musl --release -p vsd
    cross build --target aarch64-unknown-linux-musl --release -p vsd
    cross build --target x86_64-pc-windows-gnu --release -p vsd

# Build with zigbuild for cross-compilation
zigbuild target:
    cargo zigbuild -p vsd --release --target {{target}} --no-default-features --features "browser,rustls-tls-webpki-roots"

# Build all Linux targets with zigbuild
zigbuild-linux:
    just zigbuild x86_64-unknown-linux-musl
    just zigbuild aarch64-unknown-linux-musl

# Build Bento4 submodule
build-bento4:
    cd bento4-src/Bento4 && mkdir -p cmakebuild && cd cmakebuild && cmake -DCMAKE_BUILD_TYPE=Release .. && make

# Update submodules
update-submodules:
    git submodule update --init --recursive

# Run with example
run *args:
    cargo run -p vsd -- {{args}}

# Profile build (requires cargo-profiling tools)
profile:
    cargo build --release
    perf record --call-graph=dwarf target/release/vsd --help
    perf report

# Security audit
audit:
    cargo audit

# Check for outdated dependencies
outdated:
    cargo outdated

# Generate coverage report (requires cargo-tarpaulin)
coverage:
    cargo tarpaulin --all-features --workspace --timeout 120 --out Html

# Prepare for release
prepare-release: clean fmt clippy test build-release
    @echo "Ready for release!"

# CI pipeline
ci: fmt clippy test build

# Development setup
setup:
    rustup component add rustfmt clippy
    cargo install cargo-audit cargo-outdated cargo-tarpaulin
    just update-submodules

# # Install build dependencies
# install-deps:
#     @echo "Installing build dependencies..."
#     sudo apt update
#     sudo apt install -y build-essential libssl-dev pkg-config protobuf-compiler
#     @echo "Installing Rust toolchain..."
#     rustup component add rustfmt clippy
#     @echo "Installing cargo tools..."
#     cargo install cargo-audit cargo-outdated cargo-tarpaulin cargo-zigbuild cargo-xwin
#     @echo "Adding cross-compilation targets..."
#     rustup target add aarch64-apple-darwin aarch64-linux-android aarch64-pc-windows-msvc aarch64-unknown-linux-musl x86_64-apple-darwin x86_64-pc-windows-msvc x86_64-unknown-linux-musl
#     just update-submodules
#     @echo "Dependencies installed successfully!"

# Install build dependencies
install-deps:
    @echo "Installing build dependencies..."
    sudo apt update
    sudo apt install -y build-essential libssl-dev pkg-config protobuf-compiler
    just update-submodules
    @echo "Dependencies installed successfully!"