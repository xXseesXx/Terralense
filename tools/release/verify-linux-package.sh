#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf 'Usage: %s <terralens-<version>-linux-x86_64.tar.gz>\n' "$0" >&2
  exit 2
fi
package_archive="$(realpath -- "$1")"
package_dir="$(dirname -- "$package_archive")"
package_file="$(basename -- "$package_archive")"
package_name="${package_file%.tar.gz}"
source_archive="$package_dir/$package_name-source.tar.gz"
checksum_file="$package_dir/$package_name.sha256"

test -f "$source_archive"
test -f "$checksum_file"
(
  cd -- "$package_dir"
  sha256sum --check --strict "$package_name.sha256"
)

tmp_dir="$(mktemp -d)"
trap 'rm -rf -- "$tmp_dir"' EXIT
tar -xzf "$package_archive" -C "$tmp_dir"
tar -xzf "$source_archive" -C "$tmp_dir"
package_root="$tmp_dir/$package_name"
source_root="$tmp_dir/$package_name-source"
test -x "$package_root/terralens"
test -x "$package_root/install.sh"
test -f "$package_root/LICENSE"
test -f "$package_root/NOTICE.md"
test -f "$package_root/THIRD-PARTY-NOTICES.md"
test -f "$package_root/assets/terralens.svg"

(cd -- "$source_root" && sha256sum --check --strict SOURCE-MANIFEST.sha256 >/dev/null)
test -f "$source_root/.cargo/config.toml"
test -d "$source_root/native/terralens-rs/vendor"

version="${package_name#terralens-}"
version="${version%-linux-x86_64}"
"$package_root/terralens" --version | grep -Fx "TerraLens $version"
"$package_root/terralens" --help > "$tmp_dir/help.txt"
grep -Fq 'Usage: terralens' "$tmp_dir/help.txt"
"$package_root/terralens" --build-info > "$tmp_dir/build-info.json"
python3 - "$tmp_dir/build-info.json" <<'PY'
import json, sys
info = json.load(open(sys.argv[1], encoding='utf-8'))
assert info.get('official') is True, info
assert info.get('diagnostics') is False, info
assert info.get('optional_game_artwork') is False, info
assert info.get('target') == 'x86_64-unknown-linux-gnu', info
PY

# Exercise the package installer with isolated XDG/HOME locations. This verifies
# the generated launcher/icon paths without touching the user's desktop setup.
isolated_home="$tmp_dir/isolated home"
mkdir -p "$isolated_home"
HOME="$isolated_home" \
XDG_DATA_HOME="$isolated_home/.local/share" \
XDG_BIN_HOME="$isolated_home/.local/bin" \
  bash "$package_root/install.sh"
HOME="$isolated_home" \
XDG_DATA_HOME="$isolated_home/.local/share" \
XDG_BIN_HOME="$isolated_home/.local/bin" \
  bash "$package_root/install.sh"
test -L "$isolated_home/.local/bin/terralens"
test "$(readlink "$isolated_home/.local/bin/terralens")" = "$isolated_home/.local/share/terralens/$version/terralens"
installed_root="$isolated_home/.local/share/terralens/$version"
grep -Fq "Exec=\"$installed_root/terralens\"" "$isolated_home/.local/share/applications/terralens.desktop"
test -f "$installed_root/LICENSE"
test -f "$installed_root/NOTICE.md"
test -f "$installed_root/SOURCE.txt"
test -f "$installed_root/THIRD-PARTY-NOTICES.md"
test -d "$installed_root/licenses/rust"
test ! -e "$installed_root/licenses/licenses"
test -d "$installed_root/third-party-licenses"
test -f "$installed_root/provenance/ore-icons/README.md"
test -f "$installed_root/provenance/structure-icons/README.md"

# Prove that the included complete source builds using only vendored crates.
(cd -- "$source_root" && TERRALENS_OFFICIAL_BUILD=1 CARGO_TARGET_DIR="$tmp_dir/source-target" cargo build --release --offline --locked --manifest-path native/terralens-rs/Cargo.toml)
source_binary="$tmp_dir/source-target/release/terralens"
"$source_binary" --version | grep -Fx "TerraLens $version"
"$source_binary" --build-info > "$tmp_dir/source-build-info.json"
python3 - "$tmp_dir/source-build-info.json" <<'PY'
import json, sys
info = json.load(open(sys.argv[1], encoding='utf-8'))
assert info.get('official') is True, info
assert info.get('diagnostics') is False, info
assert info.get('optional_game_artwork') is False, info
PY

printf 'Verified package archive, source hashes, isolated installer and offline source build.\n'
