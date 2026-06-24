#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# pre-commit.sh — Shoggoth Backbone Pre-Commit Hook
#
# Install: cp scripts/pre-commit.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
#
# Runs before every commit:
#   1. cargo fmt --check (Rust formatting)
#   2. cargo clippy -- -D warnings (Rust lint)
#   3. cargo test --workspace (all unit tests)
#   4. ruff check (Python lint, if NPU-STACK is present)
#
# Skips on --no-verify (`git commit -n`).

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASS=0
FAIL=0

echo -e "${YELLOW}=== Shoggoth Pre-Commit Hook ===${NC}"

# ── 1. Rust Format Check ──────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}[1/4] Checking Rust formatting...${NC}"
if cargo fmt --all --manifest-path shoggoth-backbone/Cargo.toml -- --check; then
    echo -e "${GREEN}  ✓ Rust formatting OK${NC}"
    PASS=$((PASS + 1))
else
    echo -e "${RED}  ✗ Rust formatting errors. Run 'cargo fmt --all' to fix.${NC}"
    FAIL=$((FAIL + 1))
fi

# ── 2. Rust Lint ──────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}[2/4] Running cargo clippy...${NC}"
if cargo clippy --workspace --all-targets --manifest-path shoggoth-backbone/Cargo.toml -- -D warnings 2>/dev/null; then
    echo -e "${GREEN}  ✓ Clippy clean${NC}"
    PASS=$((PASS + 1))
else
    echo -e "${RED}  ✗ Clippy warnings. Fix before committing.${NC}"
    FAIL=$((FAIL + 1))
fi

# ── 3. Rust Tests ─────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}[3/4] Running cargo test...${NC}"
if cargo test --workspace --manifest-path shoggoth-backbone/Cargo.toml 2>/dev/null; then
    echo -e "${GREEN}  ✓ All tests pass${NC}"
    PASS=$((PASS + 1))
else
    echo -e "${RED}  ✗ Test failures. Fix before committing.${NC}"
    FAIL=$((FAIL + 1))
fi

# ── 4. Python Lint (optional) ─────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}[4/4] Checking Python formatting...${NC}"
if command -v ruff &> /dev/null && [ -d "npu-stack" ]; then
    if ruff check npu-stack/ 2>/dev/null; then
        echo -e "${GREEN}  ✓ Python lint clean${NC}"
        PASS=$((PASS + 1))
    else
        echo -e "${RED}  ✗ Python lint errors. Run 'ruff format npu-stack/' to fix.${NC}"
        FAIL=$((FAIL + 1))
    fi
else
    echo -e "${YELLOW}  - ruff not installed or npu-stack not present — skipping${NC}"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}=== Pre-Commit Summary: ${PASS} passed, ${FAIL} failed ===${NC}"

if [ "$FAIL" -gt 0 ]; then
    echo -e "${RED}Commit blocked. Fix the issues above and try again.${NC}"
    echo -e "${YELLOW}Tip: use 'git commit -n' to bypass hooks (not recommended).${NC}"
    exit 1
fi

echo -e "${GREEN}All checks passed. Proceeding with commit.${NC}"
exit 0
