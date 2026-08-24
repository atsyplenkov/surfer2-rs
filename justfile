# Default: show available targets.
default:
    @just --list

# Run cargo clippy and cargo fmt
lint:
  cargo clippy \
    --all-targets \
    --all-features \
    --locked \
    -- \
    -D warnings \
    -D clippy::dbg_macro

  cargo fmt

# Run the full test suite in release (lake10623 is ~2 min in debug)
test:
  cargo test --release --locked -- --test-threads=2

# Run the magadan Criterion benchmarks (see README for per-target filters)
bench:
  cargo bench --bench magadan

# Apply fixes reported by `just lint`
lint-fix:
  cargo clippy \
    --all-targets \
    --all-features \
    --locked \
    --fix --allow-dirty

  cargo fmt
