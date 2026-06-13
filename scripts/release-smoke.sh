#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/app"

cargo fmt -- --check
cargo check --locked
cargo clippy --locked -- -D warnings
cargo test --locked
cargo build --release --locked
