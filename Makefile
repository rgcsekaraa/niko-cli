BINARY_NAME=niko
VERSION?=$(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")

DIST_DIR=dist

.PHONY: all build clean test install uninstall release help

all: build

build:
	cargo build --release

build-fast:
	cargo build

install: build
	sudo cp target/release/$(BINARY_NAME) /usr/local/bin/$(BINARY_NAME)

uninstall:
	sudo rm -f /usr/local/bin/$(BINARY_NAME)

clean:
	cargo clean
	rm -rf $(DIST_DIR)

test:
	cargo test

lint:
	cargo clippy -- -D warnings

deps:
	cargo fetch

# Cross-compilation targets
$(DIST_DIR):
	mkdir -p $(DIST_DIR)

build-linux-amd64: $(DIST_DIR)
	cross build --release --target x86_64-unknown-linux-gnu
	cp target/x86_64-unknown-linux-gnu/release/$(BINARY_NAME) $(DIST_DIR)/$(BINARY_NAME)-linux-amd64

build-linux-arm64: $(DIST_DIR)
	cross build --release --target aarch64-unknown-linux-gnu
	cp target/aarch64-unknown-linux-gnu/release/$(BINARY_NAME) $(DIST_DIR)/$(BINARY_NAME)-linux-arm64

build-darwin-amd64: $(DIST_DIR)
	cargo build --release --target x86_64-apple-darwin
	cp target/x86_64-apple-darwin/release/$(BINARY_NAME) $(DIST_DIR)/$(BINARY_NAME)-darwin-amd64

build-darwin-arm64: $(DIST_DIR)
	cargo build --release --target aarch64-apple-darwin
	cp target/aarch64-apple-darwin/release/$(BINARY_NAME) $(DIST_DIR)/$(BINARY_NAME)-darwin-arm64

release: clean build-linux-amd64 build-linux-arm64 build-darwin-amd64 build-darwin-arm64
	@echo "Built all release binaries in $(DIST_DIR)/"
	@ls -la $(DIST_DIR)/

checksums: release
	cd $(DIST_DIR) && sha256sum * > checksums.txt 2>/dev/null || shasum -a 256 * > checksums.txt
	@cat $(DIST_DIR)/checksums.txt

# Development helpers
dev: build-fast
	./target/debug/$(BINARY_NAME)

run:
	cargo run -- $(ARGS)

watch:
	@which cargo-watch > /dev/null || (echo "Install: cargo install cargo-watch" && exit 1)
	cargo watch -x build

help:
	@echo "Niko CLI Makefile (Rust)"
	@echo ""
	@echo "Usage:"
	@echo "  make build        Build release binary"
	@echo "  make build-fast   Build debug binary (faster)"
	@echo "  make install      Build and install to /usr/local/bin"
	@echo "  make uninstall    Remove from /usr/local/bin"
	@echo "  make clean        Remove build artifacts"
	@echo "  make test         Run tests"
	@echo "  make lint         Run clippy linter"
	@echo "  make deps         Download dependencies"
	@echo "  make release      Build for all platforms"
	@echo "  make checksums    Create SHA256 checksums for releases"
	@echo "  make dev          Build debug and run"
	@echo "  make run ARGS='cmd find files'  Run without building"
