#!/usr/bin/env bash
set -euo pipefail
terralens_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cargo build --release --locked --manifest-path "$terralens_root/native/terralens-rs/Cargo.toml"
printf 'Built %s\n' "$terralens_root/native/terralens-rs/target/release/terralens"
