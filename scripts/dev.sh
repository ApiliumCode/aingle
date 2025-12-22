#!/usr/bin/env bash
# AIngle Development CLI
# Provides common development commands

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# ============================================================================
# Commands
# ============================================================================

cmd_help() {
    echo ""
    echo "AIngle Development CLI"
    echo "====================="
    echo ""
    echo "Usage: ./scripts/dev.sh <command> [options]"
    echo ""
    echo "Commands:"
    echo "  setup         Full development environment setup"
    echo "  build         Build the project"
    echo "  test          Run tests"
    echo "  run           Run AIngle node"
    echo "  run-iot       Run in IoT mode (sub-second)"
    echo "  run-minimal   Run minimal IoT node"
    echo "  check         Run cargo check"
    echo "  lint          Run clippy"
    echo "  fmt           Format code"
    echo "  clean         Clean build artifacts"
    echo "  docs          Generate documentation"
    echo "  stats         Show project statistics"
    echo "  dag           DAG utilities"
    echo "  template      Create from template"
    echo ""
    echo "Examples:"
    echo "  ./scripts/dev.sh run-iot"
    echo "  ./scripts/dev.sh template iot-sensor my-sensor"
    echo "  ./scripts/dev.sh stats"
    echo ""
}

cmd_setup() {
    log_info "Running full setup..."
    "$SCRIPT_DIR/setup.sh" --full
}

cmd_build() {
    local mode="${1:-debug}"
    log_info "Building in $mode mode..."

    if [[ "$mode" == "release" ]]; then
        cargo build --release
    else
        cargo build
    fi

    log_success "Build complete"
}

cmd_test() {
    local scope="${1:-workspace}"
    log_info "Running tests..."

    case "$scope" in
        workspace)
            cargo test --workspace --lib
            ;;
        all)
            cargo test --workspace
            ;;
        minimal)
            cargo test -p aingle_minimal
            ;;
        adk)
            cargo test -p adk
            ;;
        *)
            cargo test -p "$scope"
            ;;
    esac

    log_success "Tests passed"
}

cmd_run() {
    log_info "Starting AIngle node..."
    cargo run --bin aingle
}

cmd_run_iot() {
    log_info "Starting AIngle node in IoT mode..."
    AINGLE_PUBLISH_INTERVAL_MS=0 \
    AINGLE_GOSSIP_LOOP_ITERATION_DELAY_MS=100 \
    RUST_LOG=info \
    cargo run --bin aingle
}

cmd_run_minimal() {
    log_info "Building minimal node..."
    cargo build --release -p aingle_minimal

    log_info "Starting minimal IoT node..."
    AINGLE_IOT_MODE=1 \
    RUST_LOG=info \
    ./target/release/aingle-minimal
}

cmd_check() {
    log_info "Running cargo check..."
    cargo check --workspace
    log_success "Check passed"
}

cmd_lint() {
    log_info "Running clippy..."
    cargo clippy --workspace --all-targets -- -D warnings
    log_success "Lint passed"
}

cmd_fmt() {
    local check_only="${1:-false}"

    if [[ "$check_only" == "check" ]]; then
        log_info "Checking formatting..."
        cargo fmt --all -- --check
    else
        log_info "Formatting code..."
        cargo fmt --all
    fi

    log_success "Format complete"
}

cmd_clean() {
    log_info "Cleaning build artifacts..."
    cargo clean
    rm -rf data/ *.db
    log_success "Clean complete"
}

cmd_docs() {
    log_info "Generating documentation..."
    cargo doc --workspace --no-deps

    if [[ "$1" == "open" ]]; then
        cargo doc --workspace --no-deps --open
    fi

    log_success "Documentation generated"
}

cmd_stats() {
    echo ""
    echo "Project Statistics"
    echo "=================="
    echo ""

    # Line counts
    echo "Source Lines of Code:"
    find "$PROJECT_ROOT/crates" -name "*.rs" -type f | xargs wc -l 2>/dev/null | tail -1

    echo ""
    echo "Crate count:"
    ls -d "$PROJECT_ROOT/crates"/*/ 2>/dev/null | wc -l

    echo ""
    echo "Template count:"
    ls -d "$PROJECT_ROOT/templates"/*/ 2>/dev/null | wc -l

    echo ""
    echo "Documentation files:"
    ls "$PROJECT_ROOT/../contexto/"*.md 2>/dev/null | wc -l

    echo ""
    echo "Build artifacts:"
    if [[ -d "$PROJECT_ROOT/target" ]]; then
        du -sh "$PROJECT_ROOT/target"
    else
        echo "  (not built)"
    fi

    echo ""
}

cmd_dag() {
    local subcmd="${1:-help}"

    case "$subcmd" in
        export)
            log_info "DAG export not yet implemented"
            log_warn "See fase4b_dag_explorer.md for design"
            ;;
        stats)
            log_info "DAG stats not yet implemented"
            ;;
        visualize)
            log_info "DAG visualization not yet implemented"
            log_warn "See fase4b_dag_explorer.md for design"
            ;;
        *)
            echo "DAG Utilities"
            echo "============="
            echo ""
            echo "Subcommands:"
            echo "  export     Export DAG to JSON/DOT"
            echo "  stats      Show DAG statistics"
            echo "  visualize  Open DAG explorer"
            echo ""
            ;;
    esac
}

cmd_template() {
    local template="${1:-}"
    local name="${2:-}"

    if [[ -z "$template" ]]; then
        echo "Available Templates"
        echo "==================="
        echo ""
        ls -1 "$PROJECT_ROOT/templates/" 2>/dev/null || echo "  (none)"
        echo ""
        echo "Usage: ./scripts/dev.sh template <template-name> <project-name>"
        echo ""
        return
    fi

    if [[ -z "$name" ]]; then
        log_error "Please provide a project name"
        echo "Usage: ./scripts/dev.sh template $template <project-name>"
        return 1
    fi

    local src="$PROJECT_ROOT/templates/$template"
    local dest="$PROJECT_ROOT/../$name"

    if [[ ! -d "$src" ]]; then
        log_error "Template not found: $template"
        echo "Available templates:"
        ls -1 "$PROJECT_ROOT/templates/"
        return 1
    fi

    if [[ -d "$dest" ]]; then
        log_error "Destination already exists: $dest"
        return 1
    fi

    log_info "Creating project from template: $template"
    cp -r "$src" "$dest"

    # Update Cargo.toml name
    if [[ -f "$dest/Cargo.toml" ]]; then
        sed -i.bak "s/name = \".*_zome\"/name = \"${name}_zome\"/" "$dest/Cargo.toml"
        rm -f "$dest/Cargo.toml.bak"
    fi

    log_success "Project created: $dest"
    echo ""
    echo "Next steps:"
    echo "  cd $dest"
    echo "  cargo build --target wasm32-unknown-unknown"
    echo ""
}

cmd_bench() {
    log_info "Running benchmarks..."
    cargo bench --workspace
    log_success "Benchmarks complete"
}

cmd_audit() {
    log_info "Running security audit..."

    if command -v cargo-audit &> /dev/null; then
        cargo audit
    else
        log_warn "cargo-audit not installed. Install with: cargo install cargo-audit"
        return 1
    fi

    log_success "Audit complete"
}

# ============================================================================
# Main
# ============================================================================

main() {
    cd "$PROJECT_ROOT"

    local cmd="${1:-help}"
    shift || true

    case "$cmd" in
        help|--help|-h)
            cmd_help
            ;;
        setup)
            cmd_setup "$@"
            ;;
        build)
            cmd_build "$@"
            ;;
        test)
            cmd_test "$@"
            ;;
        run)
            cmd_run "$@"
            ;;
        run-iot)
            cmd_run_iot "$@"
            ;;
        run-minimal)
            cmd_run_minimal "$@"
            ;;
        check)
            cmd_check "$@"
            ;;
        lint)
            cmd_lint "$@"
            ;;
        fmt)
            cmd_fmt "$@"
            ;;
        clean)
            cmd_clean "$@"
            ;;
        docs)
            cmd_docs "$@"
            ;;
        stats)
            cmd_stats "$@"
            ;;
        dag)
            cmd_dag "$@"
            ;;
        template)
            cmd_template "$@"
            ;;
        bench)
            cmd_bench "$@"
            ;;
        audit)
            cmd_audit "$@"
            ;;
        *)
            log_error "Unknown command: $cmd"
            cmd_help
            exit 1
            ;;
    esac
}

main "$@"
