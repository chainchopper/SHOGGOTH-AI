# genex-platform — AGENTS.md

## Purpose
GENEx is a proprietary, closed-source genomics processing platform that runs on the Shoggoth Mesh Machine. It provides chromosome FASTA parsing, cross-platform WebRTC vector visualization, blockchain-anchored validation escrow, and ScyllaDB shard-per-core genomic data loading.

## Ownership
- **Owner**: GENEx Contributors (private repository)
- **Language**: Rust (edition 2024, standalone crate)
- **License**: UNLICENSED (proprietary, all rights reserved)
- **Palette**: Amber `#FF9900` + Obsidian
- **Runtime Dependency**: Links against compiled `libshoggoth_core.so` and `libshoggoth_sdk.so` from the Shoggoth Backbone.

## Local Contracts
- This is a standalone crate, NOT a workspace member of `shoggoth-backbone`.
- All code is proprietary. No Apache-2.0 or MIT licensing applies.
- Must compile against Shoggoth SDK libraries at build time via `Cargo.toml` path or registry dependencies.
- `cargo check` must pass with zero errors before any commit.
- GENEx communicates with Shoggoth Backbone via Unix domain sockets (`/dev/shm/shoggoth/`) and shared memory rings.

## Work Guidance
- `clap` 4.5 for CLI argument parsing (ingest, align, escrow, load-db subcommands).
- `scylla` 0.15 crate for ScyllaDB connectivity (shard-per-core parallelism).
- `tokio` 1.43 for async I/O across all modules.
- FASTA parser uses memory-mapped I/O (`memmap2`) for multi-GB chromosome files.
- Escrow marketplace uses `blake3` for content-addressed claim verification.
- `anyhow` for error handling with context propagation.

## Verification
- `cargo check` — must pass with zero errors.
- `cargo clippy -- -D warnings` — must pass.
- All unit tests in `src/*.rs` must pass (`cargo test`).
- FASTA parser must correctly handle 10GB+ files without OOM (test with synthetic data).

## Child DOX Index
- (No child AGENTS.md files — single-crate structure with modules in `src/`)
- `src/main.rs` — CLI entry point with ingest, align, escrow, and load-db subcommands.
- `src/alignment_engine.rs` — WebRTC vector visualization generator.
- `src/fasta_parser.rs` — High-throughput FASTA sanitizer with memory-mapped I/O.
- `src/marketplace_escrow.rs` — Blockchain-anchored validation escrow with milestone tracking.
- `src/database_connector.rs` — ScyllaDB shard-per-core multi-threaded loader.
- `src/database_reader.rs` — Real ScyllaDB CQL query layer with shard-aware connection pooling.
- `src/admin_server.rs` — Axum-based superadmin API server with auth, ingest, escrow, ScyllaDB, and audit endpoints.
- `admin/index.html` — Standalone web-based superadmin panel (HTML + vanilla JS, zero dependencies).
- `Dockerfile.genex` — Multi-stage Docker build for the GENEx daemon.
- `Dockerfile.genex-admin` — Multi-stage Docker build for the GENEx admin panel.
- `docker-compose.genex.yml` — Docker Compose for GENEx + ScyllaDB + admin panel.
