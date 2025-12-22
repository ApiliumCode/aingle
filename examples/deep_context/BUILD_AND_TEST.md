# Build and Test Instructions

## Prerequisites

- Rust 1.89 or later
- Git
- Unix-like OS (Linux, macOS) or Windows with WSL

## Build

### Development Build

```bash
cd examples/deep_context
cargo build
```

Binary location: `target/debug/deep-context`

### Release Build (Optimized)

```bash
cargo build --release
```

Binary location: `target/release/deep-context`

## Test

### Run All Tests

```bash
cargo test
```

Expected output:
```
running 9 tests (unit tests in lib)
test models::tests::test_create_decision ... ok
test models::tests::test_add_alternative ... ok
test models::tests::test_decision_status ... ok
test git_integration::tests::test_extract_decision_refs ... ok
test semantic_index::tests::test_store_and_retrieve_decision ... ok
test semantic_index::tests::test_query_by_tag ... ok
test semantic_index::tests::test_query_by_text ... ok
test tests::test_init_deep_context ... ok
test tests::test_capture_decision ... ok

running 8 tests (integration tests)
test test_init_and_open ... ok
test test_capture_decision ... ok
test test_query_decisions ... ok
test test_decisions_for_file ... ok
test test_tags ... ok
test test_export_markdown ... ok
test test_statistics ... ok
test test_next_decision_id ... ok

test result: ok. 17 passed; 0 failed
```

### Run Specific Tests

```bash
# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test integration_test

# Specific test
cargo test test_capture_decision
```

### Test with Output

```bash
cargo test -- --nocapture
```

## Verify Installation

### Check Version

```bash
./target/release/deep-context --version
```

Expected: `deep-context 0.1.0`

### Check Help

```bash
./target/release/deep-context --help
```

Should show all available commands.

## Quick Functional Test

Create a test repository and try all commands:

```bash
# Setup
cd /tmp
mkdir test-repo
cd test-repo
git init
git config user.email "test@example.com"
git config user.name "Test User"

# Initialize Deep Context
/path/to/deep-context init

# Capture a decision
/path/to/deep-context capture \
  --title "Test Decision" \
  --context "Testing the system" \
  --decision "Use testing" \
  --rationale "Because we need to test" \
  --tag "test"

# Query
/path/to/deep-context query "test"

# Stats
/path/to/deep-context stats

# Show decision
/path/to/deep-context show ADR-001

# Export
/path/to/deep-context export --format markdown --output docs/

# Cleanup
cd ..
rm -rf test-repo
```

## Run Example Script

The example usage script demonstrates a complete workflow:

```bash
./example_usage.sh
```

This will:
1. Create a test repository
2. Initialize Deep Context
3. Capture multiple decisions
4. Query decisions
5. Show timeline
6. Export documentation

## Troubleshooting

### Compilation Errors

**Error: "could not compile due to X errors"**

Make sure you have:
- Rust 1.89 or later: `rustc --version`
- Updated toolchain: `rustup update`

**Error: "failed to resolve patches"**

The example uses a standalone workspace. Make sure you're in the correct directory:
```bash
cd examples/deep_context
pwd  # Should end with /examples/deep_context
```

### Test Failures

**Error: "database locked"**

Close any running `deep-context` processes:
```bash
pkill deep-context
```

**Error: "Git repository not found"**

Some tests require Git. Install Git:
```bash
# Ubuntu/Debian
sudo apt-get install git

# macOS
brew install git

# Verify
git --version
```

### Runtime Errors

**Error: "Deep Context not initialized"**

Run `deep-context init` first:
```bash
cd your-project
deep-context init
```

**Error: "Permission denied"**

Make sure the binary is executable:
```bash
chmod +x target/release/deep-context
```

**Error: "command not found: deep-context"**

Use the full path:
```bash
/path/to/examples/deep_context/target/release/deep-context
```

Or add to PATH:
```bash
export PATH="$PATH:/path/to/examples/deep_context/target/release"
```

## Performance Testing

### Benchmark Decision Capture

```bash
#!/bin/bash
# Capture 1000 decisions and measure time

cd /tmp/perf-test
git init
git config user.email "test@example.com"
git config user.name "Test User"
deep-context init

time for i in {1..1000}; do
  deep-context capture \
    --title "Decision $i" \
    --context "Context $i" \
    --decision "Decision $i" \
    --rationale "Rationale $i" \
    --tag "perf-test" > /dev/null 2>&1
done

deep-context stats
```

Expected: < 30 seconds for 1000 decisions

### Benchmark Queries

```bash
# Query all decisions
time deep-context query

# Query by tag
time deep-context query --tag perf-test

# Query by text
time deep-context query "Decision 500"
```

Expected: < 100ms for each query

## Code Quality Checks

### Format Code

```bash
cargo fmt
```

### Lint Code

```bash
cargo clippy
```

### Check for Unsafe Code

```bash
cargo geiger
```

Expected: No unsafe code blocks

### Check Dependencies

```bash
cargo tree
cargo audit
```

## Building for Distribution

### Static Binary (Linux)

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

### macOS Universal Binary

```bash
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin

cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

lipo -create \
  target/aarch64-apple-darwin/release/deep-context \
  target/x86_64-apple-darwin/release/deep-context \
  -output deep-context-universal
```

### Windows Binary

```bash
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

## Continuous Integration

Example GitHub Actions workflow:

```yaml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Build
        run: cd examples/deep_context && cargo build --release
      - name: Test
        run: cd examples/deep_context && cargo test
      - name: Clippy
        run: cd examples/deep_context && cargo clippy -- -D warnings
```

## Development Workflow

### Watch Mode

Install cargo-watch:
```bash
cargo install cargo-watch
```

Run tests on file changes:
```bash
cargo watch -x test
```

### Debug Mode

Set environment variable:
```bash
export RUST_LOG=debug
./target/debug/deep-context query "test"
```

### Profile Performance

```bash
cargo build --release
cargo install flamegraph

# Linux
sudo flamegraph ./target/release/deep-context query "test"

# macOS
cargo instruments -t "Time Profiler" --release --example
```

## Documentation

### Generate API Docs

```bash
cargo doc --no-deps --open
```

### Check Documentation

```bash
cargo doc --no-deps
```

## Summary

✅ **Build**: `cargo build --release`
✅ **Test**: `cargo test`
✅ **Run**: `./target/release/deep-context --help`
✅ **Install**: `cargo install --path .`

All tests should pass, and the binary should be ready to use!
