BINARY_NAME=niko
VERSION?=$(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
COMMIT_SHA?=$(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BUILD_DATE?=$(shell date -u +"%Y-%m-%dT%H:%M:%SZ")

LDFLAGS=-ldflags "-s -w \
	-X github.com/niko-cli/niko/internal/cli.Version=$(VERSION) \
	-X github.com/niko-cli/niko/internal/cli.CommitSHA=$(COMMIT_SHA) \
	-X github.com/niko-cli/niko/internal/cli.BuildDate=$(BUILD_DATE)"

DIST_DIR=dist

.PHONY: all build clean test lint install uninstall release

all: build

build:
	go build $(LDFLAGS) -o $(BINARY_NAME) ./cmd/niko

build-fast:
	go build -o $(BINARY_NAME) ./cmd/niko

install: build
	sudo mv $(BINARY_NAME) /usr/local/bin/$(BINARY_NAME)

uninstall:
	sudo rm -f /usr/local/bin/$(BINARY_NAME)

clean:
	rm -f $(BINARY_NAME)
	rm -rf $(DIST_DIR)

test:
	go test -v ./...

lint:
	golangci-lint run

deps:
	go mod download
	go mod tidy

# Cross-compilation targets
$(DIST_DIR):
	mkdir -p $(DIST_DIR)

build-linux-amd64: $(DIST_DIR)
	GOOS=linux GOARCH=amd64 go build $(LDFLAGS) -o $(DIST_DIR)/$(BINARY_NAME)-linux-amd64 ./cmd/niko

build-linux-arm64: $(DIST_DIR)
	GOOS=linux GOARCH=arm64 go build $(LDFLAGS) -o $(DIST_DIR)/$(BINARY_NAME)-linux-arm64 ./cmd/niko

build-darwin-amd64: $(DIST_DIR)
	GOOS=darwin GOARCH=amd64 go build $(LDFLAGS) -o $(DIST_DIR)/$(BINARY_NAME)-darwin-amd64 ./cmd/niko

build-darwin-arm64: $(DIST_DIR)
	GOOS=darwin GOARCH=arm64 go build $(LDFLAGS) -o $(DIST_DIR)/$(BINARY_NAME)-darwin-arm64 ./cmd/niko

build-windows-amd64: $(DIST_DIR)
	GOOS=windows GOARCH=amd64 go build $(LDFLAGS) -o $(DIST_DIR)/$(BINARY_NAME)-windows-amd64.exe ./cmd/niko

build-windows-arm64: $(DIST_DIR)
	GOOS=windows GOARCH=arm64 go build $(LDFLAGS) -o $(DIST_DIR)/$(BINARY_NAME)-windows-arm64.exe ./cmd/niko

release: clean build-linux-amd64 build-linux-arm64 build-darwin-amd64 build-darwin-arm64 build-windows-amd64 build-windows-arm64
	@echo "Built all release binaries in $(DIST_DIR)/"
	@ls -la $(DIST_DIR)/

# Create checksums for release
checksums: release
	cd $(DIST_DIR) && sha256sum * > checksums.txt
	@cat $(DIST_DIR)/checksums.txt

# Development helpers
dev: build-fast
	./$(BINARY_NAME)

run:
	go run ./cmd/niko $(ARGS)

watch:
	@which watchexec > /dev/null || (echo "Install watchexec: cargo install watchexec-cli" && exit 1)
	watchexec -r -e go -- make build-fast

.PHONY: help
help:
	@echo "Niko CLI Makefile"
	@echo ""
	@echo "Usage:"
	@echo "  make build        Build for current platform"
	@echo "  make build-fast   Build without version info (faster)"
	@echo "  make install      Build and install to /usr/local/bin"
	@echo "  make uninstall    Remove from /usr/local/bin"
	@echo "  make clean        Remove build artifacts"
	@echo "  make test         Run tests"
	@echo "  make lint         Run linter"
	@echo "  make deps         Download and tidy dependencies"
	@echo "  make release      Build for all platforms"
	@echo "  make checksums    Create SHA256 checksums for releases"
	@echo "  make dev          Build and run"
	@echo "  make run ARGS='your query'  Run without building"
