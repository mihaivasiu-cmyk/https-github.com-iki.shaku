#!/usr/bin/env bash
# One command before you push: format, lint, and every test.
#   scripts/check.sh            full check (needs crates.io / cached deps)
#   scripts/check.sh --offline  only the offline sound-kernel tests
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "${1:-}" == "--offline" ]]; then
  exec scripts/offline-check.sh
fi
echo "== fmt";    cargo fmt --all --check
echo "== clippy"; cargo clippy --workspace --all-targets -- -D warnings
echo "== test";   cargo test --release --workspace
echo "== offline kernel tests"; scripts/offline-check.sh
echo "all checks passed"
