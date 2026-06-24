# Contributing to Shoggoth Mesh Machine

## Code of Conduct
This project follows a simple rule: **respect the hardware, respect the fabric, respect each other.**

## Development Setup

### Prerequisites
- **Rust 1.85+** with `cargo`, `clippy`, `rustfmt`
- **Node.js 20+** (for Tauri dashboard frontend)
- **Python 3.12+** (for NPU-STACK and Python client)
- **Docker** (for container builds)

### First-Time Setup
```bash
git clone https://github.com/chainchopper/shoggoth-backbone.git
cd shoggoth-backbone

# Rust toolchain
rustup default stable
rustup component add clippy rustfmt

# Verify compilation
make check
```

## Development Loop

```bash
make check     # Cargo check entire workspace (fast)
make lint      # Cargo clippy on workspace
make test      # Run all tests
make build     # Release build
```

For faster iteration on a single crate:
```bash
cargo check -p shoggoth-core
cargo test -p shoggoth-sdk
cargo clippy -p shoggoth-display -- -D warnings
```

## Code Style

### Rust
- Edition 2024. All workspace lint rules defined in root `Cargo.toml`.
- `#[deny(unsafe_code)]` on all crates except `shoggoth-core`.
- All `unsafe` blocks require `// SAFETY:` comments.
- No `std::sync::Mutex` on hot paths — use `dashmap`, `crossbeam-channel`, or atomics.
- No blocking I/O inside `async` context.
- Format: `cargo fmt --all`.

### Python
- `ruff` for linting (`ruff check .`).
- `mypy` for type checking (`mypy backend/ kernels/ --strict`).
- Format: `ruff format .`.

### TypeScript
- `tsc --noEmit` for type checking.
- `eslint` for linting.
- `prettier` for formatting.

## Commit Guidelines

Follow [Conventional Commits](https://www.conventionalcommits.org/):
```
feat(core): add DMA-BUF zero-copy export
fix(display): handle compositor OOB
docs(planning): update Phase 6 checkboxes
test(integration): add orchestrator API tests
```

## Pull Request Checklist

- [ ] All tests pass: `make test`
- [ ] Lint clean: `make lint`
- [ ] Format consistent: `make format`
- [ ] No new `unsafe` blocks without `// SAFETY:` comments
- [ ] New modules registered in `lib.rs`
- [ ] Dependencies declared in workspace `Cargo.toml`
- [ ] DOX AGENTS.md updated if folder structure changed
- [ ] `planning.md` checkboxes updated

## Architecture Rules

1. **The Backbone knows nothing of application logic.** It only sees `ComputeTask`, `RenderTile`, `NodeHeartbeat`.
2. **Application repos are separate.** `genex-platform` and `npu-stack` are not workspace members.
3. **Lock-free first.** If you reach for `Mutex`, reconsider.
4. **Bandwidth is precious.** Never transfer model weights, raw frames, or assets over the network at runtime.

## Testing

- **Unit tests**: `cargo test -p <crate>`
- **Integration tests**: `cargo test --test '*' -- --include-ignored`
- **Load tests**: `k6 run tests/load/k6-load-test.js`

## Documentation

- SDK docs: `make docs-serve` (mdBook, port 3000).
- API docs: `cargo doc --workspace --no-deps --open`.
- Architecture: `planning.md`.

## Questions?

Open a [GitHub issue](https://github.com/chainchopper/shoggoth-backbone/issues) or start a [discussion](https://github.com/chainchopper/shoggoth-backbone/discussions).
