# Shoggoth Mesh Machine — Development Makefile
#
# Common development tasks. Requires:
#   • Rust 1.85+ with cargo, clippy, rustfmt
#   • Node.js 20+ (for dashboard frontend)
#   • Python 3.12+ (for NPU-STACK and Python client)
#   • Docker (for container builds)

.PHONY: help check lint test bench build clean \
        check-all lint-all test-all build-all \
        check-genex lint-genex test-genex \
        check-workspace lint-workspace test-workspace \
        frontend-dev frontend-build \
        docker-build docker-run \
        docs docs-serve \
        certs \
        security-audit \
        loc format

# ── Default ─────────────────────────────────────────────────────────────────────
.DEFAULT_GOAL := help

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-24s\033[0m %s\n", $$1, $$2}'

# ── Backbone Workspace ─────────────────────────────────────────────────────────

check: check-workspace ## Run cargo check on the backbone workspace
lint: lint-workspace ## Run cargo clippy on the backbone workspace
test: test-workspace ## Run all backbone tests
bench: ## Run benchmarks
	cargo bench --workspace --manifest-path shoggoth-backbone/Cargo.toml

check-workspace: ## Cargo check — entire backbone workspace
	cargo check --workspace --all-targets --manifest-path shoggoth-backbone/Cargo.toml

lint-workspace: ## Cargo clippy — entire backbone workspace
	cargo clippy --workspace --all-targets --manifest-path shoggoth-backbone/Cargo.toml -- -D warnings

test-workspace: ## Cargo test — entire backbone workspace
	cargo test --workspace --manifest-path shoggoth-backbone/Cargo.toml

build: ## Release build — entire backbone workspace
	cargo build --release --workspace --manifest-path shoggoth-backbone/Cargo.toml

# ── GENEx Platform (standalone) ─────────────────────────────────────────────────

check-genex: ## Cargo check — genex-platform
	cargo check --all-targets --manifest-path genex-platform/Cargo.toml

lint-genex: ## Cargo clippy — genex-platform
	cargo clippy --all-targets --manifest-path genex-platform/Cargo.toml -- -D warnings

test-genex: ## Cargo test — genex-platform
	cargo test --manifest-path genex-platform/Cargo.toml

# ── Combined ────────────────────────────────────────────────────────────────────

check-all: check-workspace check-genex ## Cargo check — all Rust crates
lint-all: lint-workspace lint-genex ## Cargo clippy — all Rust crates
test-all: test-workspace test-genex ## Cargo test — all Rust crates
build-all: build ## Release build — all Rust crates

# ── Frontend (Tauri Dashboard) ──────────────────────────────────────────────────

frontend-dev: ## Start Vite dev server for the dashboard
	cd shoggoth-backbone/apps/shoggoth-desktop && npm run dev

frontend-build: ## Build the dashboard frontend
	cd shoggoth-backbone/apps/shoggoth-desktop && npm run build

frontend-lint: ## Lint the dashboard frontend
	cd shoggoth-backbone/apps/shoggoth-desktop && npm run lint

# ── NPU-STACK ───────────────────────────────────────────────────────────────────

npu-lint: ## Lint NPU-STACK Python code
	cd npu-stack && ruff check .

npu-typecheck: ## Type-check NPU-STACK
	cd npu-stack && mypy backend/ kernels/

npu-test: ## Run NPU-STACK tests
	cd npu-stack && pytest -v

# ── Docker ──────────────────────────────────────────────────────────────────────

docker-build-orchestrator: ## Build orchestrator Docker image
	docker build -t ghcr.io/chainchopper/shoggoth-orchestrator:latest \
		-f shoggoth-backbone/Dockerfile.orchestrator shoggoth-backbone/

docker-build-node-agent: ## Build node agent Docker image
	docker build -t ghcr.io/chainchopper/shoggoth-node-agent:latest \
		-f shoggoth-backbone/Dockerfile.node-agent shoggoth-backbone/

docker-build-npu: ## Build NPU-STACK Docker image
	docker build -t ghcr.io/chainchopper/npu-stack:latest npu-stack/

docker-build: docker-build-orchestrator docker-build-node-agent docker-build-npu ## Build all Docker images

docker-compose-up: ## Start all services via docker-compose
	docker compose -f shoggoth-backbone/docker-compose.shoggoth.yml up -d

docker-compose-down: ## Stop all services
	docker compose -f shoggoth-backbone/docker-compose.shoggoth.yml down

# ── Certificates ────────────────────────────────────────────────────────────────

certs: ## Generate Shoggoth CA and node certificates
	bash shoggoth-backbone/scripts/setup-shoggoth-ca.sh --all-nodes 12

# ── Documentation ───────────────────────────────────────────────────────────────

docs: ## Build SDK documentation (mdBook)
	cd shoggoth-backbone && mdbook build

docs-serve: ## Serve SDK documentation locally
	cd shoggoth-backbone && mdbook serve --open

# ── Security ────────────────────────────────────────────────────────────────────

security-audit: ## Run cargo-audit on all dependencies
	cargo audit --manifest-path shoggoth-backbone/Cargo.toml
	cargo audit --manifest-path genex-platform/Cargo.toml

# ── Utilities ───────────────────────────────────────────────────────────────────

loc: ## Count lines of code
	@echo "=== Shoggoth Backbone ==="
	@tokei shoggoth-backbone/ --exclude node_modules --exclude target 2>/dev/null || \
		find shoggoth-backbone -name '*.rs' -o -name '*.ts' -o -name '*.tsx' -o -name '*.py' -o -name '*.glsl' | \
		xargs wc -l | tail -1
	@echo ""
	@echo "=== GENEx Platform ==="
	@tokei genex-platform/ 2>/dev/null || \
		find genex-platform -name '*.rs' | xargs wc -l | tail -1
	@echo ""
	@echo "=== NPU-STACK ==="
	@tokei npu-stack/ 2>/dev/null || \
		find npu-stack -name '*.py' | xargs wc -l | tail -1

format: ## Format all code
	cargo fmt --all --manifest-path shoggoth-backbone/Cargo.toml
	cargo fmt --all --manifest-path genex-platform/Cargo.toml
	cd npu-stack && ruff format .

clean: ## Clean all build artifacts
	cargo clean --manifest-path shoggoth-backbone/Cargo.toml
	cargo clean --manifest-path genex-platform/Cargo.toml
	rm -rf shoggoth-backbone/apps/shoggoth-desktop/dist
	rm -rf shoggoth-backbone/apps/shoggoth-desktop/node_modules
	rm -rf shoggoth-backbone/target
	rm -rf npu-stack/__pycache__ npu-stack/.pytest_cache npu-stack/.mypy_cache
	rm -rf npu-stack/backend/__pycache__ npu-stack/backend/routers/__pycache__
	rm -rf deploy/certs
