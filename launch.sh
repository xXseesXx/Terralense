#!/usr/bin/env bash
set -euo pipefail
terralens_root="$(cd -- "$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")" && pwd)"
terralens_binary="$terralens_root/native/terralens-rs/target/release/terralens"
if [[ ! -x "$terralens_binary" ]]; then
  printf 'Run %s/build.sh once to build TerraLens.\n' "$terralens_root" >&2
  exit 1
fi
exec "$terralens_binary" "$@"
