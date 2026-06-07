#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERIFY_SCRIPT="$PROJECT_DIR/verify.sh"

printf '\n=== Focus Guard In Action ===\n'
"$VERIFY_SCRIPT"
printf '\n=== Focus Guard In Action Demo Complete ===\n'
