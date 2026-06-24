# Shoggoth Backbone — Release Checklist & Script
#
# Follow this checklist for each release. Semantic versioning.
#
# ## Pre-Release
#
# - [ ] All CI checks green (`.github/workflows/shoggoth-ci.yml`).
# - [ ] `cargo test --workspace` passes.
# - [ ] `cargo clippy --workspace -- -D warnings` clean.
# - [ ] `cargo fmt --all -- --check` clean.
# - [ ] `cargo audit` clean (no critical CVEs).
# - [ ] `cargo deny check` clean.
# - [ ] CHANGELOG.md updated with this version's entries.
# - [ ] Version bumped in all Cargo.toml files (`[workspace.package] version`).
# - [ ] OpenAPI spec matches current API surface.
# - [ ] `npu-stack` integration test passed against live orchestrator.
#
# ## Release Steps
#
# ```bash
# # 1. Dry run
# ./scripts/release.sh --dry-run 0.1.0
#
# # 2. Actual release
# ./scripts/release.sh 0.1.0
# ```
#
# ## Post-Release
#
# - [ ] Git tag created: `git tag v0.1.0 && git push origin v0.1.0`.
# - [ ] GitHub Release created with CHANGELOG.md contents.
# - [ ] Docker images pushed to `ghcr.io/chainchopper`.
# - [ ] crates.io publish for `shoggoth-sdk` and `shoggoth-core`.
# - [ ] npm publish for `@shoggoth/client`.
# - [ ] PyPI publish for `shoggoth-client`.
# - [ ] Update documentation site.

#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
DRY_RUN="${2:-false}"

if [ "$1" = "--dry-run" ]; then
    DRY_RUN="true"
    VERSION="${2:-}"
fi

if [ -z "$VERSION" ]; then
    echo "Usage: $0 [--dry-run] <version>"
    echo "Example: $0 0.1.0"
    exit 1
fi

echo "=== Shoggoth Release: v${VERSION} ==="
echo "Dry run: ${DRY_RUN}"
echo ""

# ── 1. Verify workspace is clean ──────────────────────────────────────────────
if [ -n "$(git status --porcelain)" ]; then
    echo "ERROR: Working directory is not clean. Commit or stash changes first."
    exit 1
fi

# ── 2. Update version in all Cargo.toml files ─────────────────────────────────
echo "[1/6] Updating version to ${VERSION}..."
if [ "$DRY_RUN" = "true" ]; then
    echo "  (dry run) Would update Cargo.toml to ${VERSION}"
else
    # Update workspace version.
    sed -i "s/^version = \".*\"/version = \"${VERSION}\"/" shoggoth-backbone/Cargo.toml
    echo "  Updated shoggoth-backbone/Cargo.toml"
fi

# ── 3. Run final tests ────────────────────────────────────────────────────────
echo "[2/6] Running final test suite..."
if [ "$DRY_RUN" = "true" ]; then
    echo "  (dry run) Would run: cargo test --workspace"
else
    cargo test --workspace --manifest-path shoggoth-backbone/Cargo.toml
    echo "  All tests pass"
fi

# ── 4. Build release binaries ─────────────────────────────────────────────────
echo "[3/6] Building release binaries..."
if [ "$DRY_RUN" = "true" ]; then
    echo "  (dry run) Would run: cargo build --release --workspace"
else
    cargo build --release --workspace --manifest-path shoggoth-backbone/Cargo.toml
    echo "  Release build complete"
fi

# ── 5. Build Docker images ────────────────────────────────────────────────────
echo "[4/6] Building Docker images..."
if [ "$DRY_RUN" = "true" ]; then
    echo "  (dry run) Would build docker images"
else
    docker build -t "ghcr.io/chainchopper/shoggoth-orchestrator:${VERSION}" \
        -f shoggoth-backbone/Dockerfile.orchestrator shoggoth-backbone/
    docker build -t "ghcr.io/chainchopper/shoggoth-node-agent:${VERSION}" \
        -f shoggoth-backbone/Dockerfile.node-agent shoggoth-backbone/
    echo "  Docker images built"
fi

# ── 6. Git tag ────────────────────────────────────────────────────────────────
echo "[5/6] Creating git tag v${VERSION}..."
if [ "$DRY_RUN" = "true" ]; then
    echo "  (dry run) Would create git tag v${VERSION}"
else
    git add shoggoth-backbone/Cargo.toml
    git commit -m "chore: release v${VERSION}"
    git tag -a "v${VERSION}" -m "Shoggoth v${VERSION}"
    echo "  Tag created: v${VERSION}"
fi

# ── Push ──────────────────────────────────────────────────────────────────────
echo "[6/6] Pushing to origin..."
if [ "$DRY_RUN" = "true" ]; then
    echo "  (dry run) Would push to origin"
else
    git push origin main
    git push origin "v${VERSION}"
    docker push "ghcr.io/chainchopper/shoggoth-orchestrator:${VERSION}"
    docker push "ghcr.io/chainchopper/shoggoth-node-agent:${VERSION}"
    echo "  Pushed to origin and registry"
fi

echo ""
echo "=== Release v${VERSION} Complete ==="
echo "Next steps:"
echo "  1. Create GitHub Release with CHANGELOG.md entries."
echo "  2. cargo publish -p shoggoth-sdk"
echo "  3. cargo publish -p shoggoth-core"
echo "  4. cd clients/typescript && npm publish"
echo "  5. cd clients/python && python -m build && twine upload dist/*"
