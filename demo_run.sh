#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERIFY_SCRIPT="$PROJECT_DIR/verify.sh"
CARGO_TOML="$PROJECT_DIR/Cargo.toml"

printf '\n=== Focus Guard Demo: Runtime Verification ===\n'
"$VERIFY_SCRIPT"

printf '\n=== Focus Guard Demo: Unit Tests ===\n'
cargo test --manifest-path "$CARGO_TOML"

printf '\n=== Demo Complete ===\n'
