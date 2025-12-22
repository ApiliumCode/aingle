# AIngle Development Makefile
# Version: 2.0.0

.PHONY: all setup build build-release test test-all clean lint fmt check docs run run-iot help

# Default target
all: help

# ============================================================================
# Setup
# ============================================================================

setup: ## Full development environment setup
	@./scripts/setup.sh --full

setup-deps: ## Install dependencies only
	@./scripts/setup.sh --deps-only

setup-iot: ## Setup IoT mode configuration
	@./scripts/setup.sh --iot

# ============================================================================
# Build
# ============================================================================

build: ## Build in debug mode
	cargo build

build-release: ## Build in release mode (optimized)
	cargo build --release

build-adk: ## Build ADK only
	cargo build -p adk

build-minimal: ## Build minimal IoT node
	cargo build --release --features minimal -p aingle

# ============================================================================
# Testing
# ============================================================================

test: ## Run unit tests
	cargo test --workspace --lib

test-all: ## Run all tests including integration
	cargo test --workspace

test-adk: ## Run ADK tests
	cargo test -p adk

test-p2p: ## Run P2P networking tests
	cargo test -p kitsune_p2p

# ============================================================================
# Code Quality
# ============================================================================

lint: ## Run clippy linter
	cargo clippy --workspace --all-targets -- -D warnings

fmt: ## Format code
	cargo fmt --all

fmt-check: ## Check formatting without changes
	cargo fmt --all -- --check

check: ## Run cargo check
	cargo check --workspace

audit: ## Security audit dependencies
	cargo audit

# ============================================================================
# Documentation
# ============================================================================

docs: ## Generate documentation
	cargo doc --workspace --no-deps

docs-open: ## Generate and open documentation
	cargo doc --workspace --no-deps --open

# ============================================================================
# Running
# ============================================================================

run: ## Run AIngle node (default mode)
	cargo run --bin aingle

run-iot: ## Run AIngle node in IoT mode (sub-second confirmation)
	AINGLE_PUBLISH_INTERVAL_MS=0 \
	AINGLE_GOSSIP_LOOP_ITERATION_DELAY_MS=100 \
	cargo run --bin aingle

run-minimal: ## Run minimal IoT node
	cargo build --release -p aingle_minimal && \
	AINGLE_IOT_MODE=1 ./target/release/aingle-minimal

run-sandbox: ## Run AI sandbox
	cargo run --bin ai_sandbox

# ============================================================================
# Cleaning
# ============================================================================

clean: ## Clean build artifacts
	cargo clean

clean-all: ## Clean everything including data
	cargo clean
	rm -rf data/
	rm -f .env.local .env.iot

# ============================================================================
# Development Utilities
# ============================================================================

watch: ## Watch for changes and rebuild
	cargo watch -x build

watch-test: ## Watch for changes and run tests
	cargo watch -x 'test --workspace --lib'

bench: ## Run benchmarks
	cargo bench --workspace

# ============================================================================
# WASM
# ============================================================================

wasm-build: ## Build WASM targets
	cargo build --target wasm32-unknown-unknown -p adk

wasm-check: ## Check WASM targets compile
	cargo check --target wasm32-unknown-unknown -p adk

# ============================================================================
# CI/CD
# ============================================================================

ci: fmt-check lint test ## Run CI checks locally
	@echo "All CI checks passed!"

ci-full: fmt-check lint test-all audit ## Run full CI checks
	@echo "All full CI checks passed!"

# ============================================================================
# Help
# ============================================================================

help: ## Show this help message
	@echo ""
	@echo "AIngle Development Commands"
	@echo "=========================="
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "Examples:"
	@echo "  make setup      # Full development setup"
	@echo "  make build      # Build in debug mode"
	@echo "  make test       # Run tests"
	@echo "  make run-iot    # Run in IoT mode"
	@echo ""
