#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="$repo_root/native/terralens-rs/target/release/terralens"
output_dir="${1:-$repo_root/dist/release}"
target="${TERRALENS_TARGET:-x86_64-unknown-linux-gnu}"

if [[ "$target" != x86_64-unknown-linux-gnu ]]; then
  printf 'Unsupported release target: %s (this package is Linux x86_64 only)\n' "$target" >&2
  exit 1
fi

version="$(python3 - "$repo_root/native/terralens-rs/Cargo.toml" <<'PY'
import sys, tomllib
with open(sys.argv[1], 'rb') as cargo_toml:
    print(tomllib.load(cargo_toml)['package']['version'])
PY
)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf -- "$tmp_dir"' EXIT

# Capture tracked plus non-ignored untracked inputs before compilation. At the
# end we reject concurrent source edits instead of pairing a binary with a
# source archive from a different tree state.
python3 - "$repo_root" "$tmp_dir/source-before.sha256" <<'PY'
import hashlib, pathlib, subprocess, sys
root = pathlib.Path(sys.argv[1]).resolve()
raw = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z', '--cached', '--others', '--exclude-standard'])
paths = sorted({p.decode() for p in raw.split(b'\0') if p and not p.decode().startswith('dist/')})
lines = []
for rel in paths:
    path = root / rel
    if path.is_file() and not path.is_symlink():
        lines.append(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {rel}\n')
pathlib.Path(sys.argv[2]).write_text(''.join(lines), encoding='utf-8')
PY

# The official build contract disables optional artwork sourced from a local
# game installation, even if that file is present in the builder's checkout.
TERRALENS_OFFICIAL_BUILD=1 cargo build --release --locked \
  --manifest-path "$repo_root/native/terralens-rs/Cargo.toml"
if [[ ! -x "$binary" ]]; then
  printf 'Release build did not produce an executable: %s\n' "$binary" >&2
  exit 1
fi
machine="$(readelf -h "$binary" | sed -n 's/^[[:space:]]*Machine:[[:space:]]*//p')"
if [[ "$machine" != 'Advanced Micro Devices X86-64' ]]; then
  printf 'Expected an x86_64 ELF binary, found: %s\n' "${machine:-unknown}" >&2
  exit 1
fi

mkdir -p -- "$output_dir"
output_dir="$(cd -- "$output_dir" && pwd)"

package_name="terralens-$version-linux-x86_64"

# Vendor every locked target dependency before collecting notices. Fresh CI
# runners do not have sources for foreign-platform crates in Cargo's cache.
source_overlay="$tmp_dir/source-overlay"
mkdir -p "$source_overlay/.cargo" "$source_overlay/native/terralens-rs/vendor"
(cd -- "$repo_root" && cargo vendor --locked \
  --manifest-path "$repo_root/native/terralens-rs/Cargo.toml" \
  "$source_overlay/native/terralens-rs/vendor" > "$tmp_dir/vendor-config.txt")
python3 - "$tmp_dir/vendor-config.txt" "$source_overlay/.cargo/config.toml" <<'PY'
import pathlib, re, sys
text = pathlib.Path(sys.argv[1]).read_text(encoding='utf-8')
start = text.find('[source.crates-io]')
if start < 0:
    raise SystemExit('cargo vendor did not return a crates.io source configuration')
config = text[start:]
config = re.sub(r'(?m)^directory\s*=\s*"[^"]+"\s*$', 'directory = "native/terralens-rs/vendor"', config)
pathlib.Path(sys.argv[2]).write_text(config.strip() + '\n', encoding='utf-8')
PY

stage="$tmp_dir/$package_name"
mkdir -p \
  "$stage/licenses" \
  "$stage/assets" \
  "$stage/provenance/ore-icons" \
  "$stage/provenance/structure-icons/cubiomes" \
  "$stage/provenance/map-colors" \
  "$stage/provenance/ground-colors"
install -m 0755 "$binary" "$stage/terralens"
install -m 0644 "$repo_root/LICENSE" "$repo_root/NOTICE.md" "$repo_root/README.md" "$stage/"
cp -a "$repo_root/licenses/." "$stage/licenses/"
install -m 0644 "$repo_root/assets/terralens.svg" "$stage/assets/terralens.svg"
install -m 0644 \
  "$repo_root/native/terralens-rs/assets/export-font-source.txt" \
  "$stage/provenance/export-font-source.txt"
install -m 0644 \
  "$repo_root/native/terralens-rs/assets/ore-icons/README.md" \
  "$stage/provenance/ore-icons/README.md"
install -m 0644 \
  "$repo_root/native/terralens-rs/assets/structure-icons/README.md" \
  "$stage/provenance/structure-icons/README.md"
install -m 0644 \
  "$repo_root/native/terralens-rs/assets/structure-icons/cubiomes/provenance.json" \
  "$stage/provenance/structure-icons/cubiomes/provenance.json"
install -m 0644 \
  "$repo_root/native/terralens-rs/assets/map-colors/README.md" \
  "$stage/provenance/map-colors/README.md"
install -m 0644 \
  "$repo_root/native/terralens-rs/assets/ground-colors/README.md" \
  "$stage/provenance/ground-colors/README.md"
python3 "$repo_root/tools/release/collect-third-party-licenses.py" \
  "$repo_root/native/terralens-rs" "$stage/third-party-licenses" \
  "$source_overlay/native/terralens-rs/vendor"

# Check the ordinary production CLI without starting the GUI.
"$stage/terralens" --version | grep -Fx "TerraLens $version"
"$stage/terralens" --help > "$tmp_dir/help.txt"
grep -Fq 'Usage: terralens' "$tmp_dir/help.txt"
"$stage/terralens" --build-info > "$tmp_dir/build-info.json"
python3 - "$tmp_dir/build-info.json" "$version" "$target" <<'PY'
import json, sys
info = json.load(open(sys.argv[1], encoding='utf-8'))
expected_version, expected_target = sys.argv[2:]
assert info.get('version') == expected_version, info
assert info.get('target') == expected_target, info
assert info.get('official') is True, info
assert info.get('diagnostics') is False, info
assert info.get('optional_game_artwork') is False, info
PY

commit="$(git -C "$repo_root" rev-parse HEAD)"
tree_state="dirty"
if [[ -z "$(git -C "$repo_root" status --porcelain --untracked-files=all -- . ':(exclude)dist/**')" ]]; then
  tree_state="clean"
fi
rustc_version="${TERRALENS_RUSTC_VERSION:-$(rustc --version 2>/dev/null || printf 'unknown')}"
built_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
glibc_symbol="$(readelf --version-info "$stage/terralens" 2>/dev/null | grep -Eo 'GLIBC_[0-9]+\.[0-9]+' | sort -Vu | tail -n 1 || true)"
glibc_min="${glibc_symbol#GLIBC_}"
glibc_min="${glibc_min:-unknown}"
build_host="$(. /etc/os-release 2>/dev/null && printf '%s %s' "${PRETTY_NAME:-Linux}" "$(uname -m)" || printf 'Linux %s' "$(uname -m)")"
cat > "$stage/SOURCE.txt" <<EOF
TerraLens $version Linux x86_64 release package
Repository: https://github.com/xXseesXx/Terralense
Git commit: $commit
Working tree: $tree_state
Build target: $target
Rust compiler: $rustc_version
Build host: $build_host
Minimum referenced glibc ABI version: $glibc_min
Package timestamp: $built_at

The matching complete source tree is supplied as $package_name-source.tar.gz.
SOURCE-MANIFEST.sha256 inside that archive records hashes of every source input
included in the archive. The working tree may include uncommitted changes; the
commit field alone does not identify the packaged source.
EOF

cat > "$stage/install.sh" <<'INSTALL'
#!/usr/bin/env bash
set -euo pipefail
package_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
install_root="${XDG_DATA_HOME:-$HOME/.local/share}/terralens"
bin_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
app_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icon_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
version="$(sed -n 's/^TerraLens \([^ ]*\) Linux.*/\1/p' "$package_dir/SOURCE.txt")"
destination="$install_root/$version"
mkdir -p -- "$destination" "$bin_dir" "$app_dir" "$icon_dir"
install -m 0755 "$package_dir/terralens" "$destination/terralens"
install -m 0644 \
  "$package_dir/LICENSE" \
  "$package_dir/NOTICE.md" \
  "$package_dir/README.md" \
  "$package_dir/SOURCE.txt" \
  "$package_dir/SOURCE-MANIFEST.sha256" \
  "$package_dir/THIRD-PARTY-NOTICES.md" \
  "$destination/"
mkdir -p -- "$destination/licenses" "$destination/third-party-licenses" "$destination/provenance"
cp -a "$package_dir/licenses/." "$destination/licenses/"
cp -a "$package_dir/third-party-licenses/." "$destination/third-party-licenses/"
cp -a "$package_dir/provenance/." "$destination/provenance/"
install -m 0644 "$package_dir/assets/terralens.svg" "$icon_dir/terralens.svg"
ln -sfn -- "$destination/terralens" "$bin_dir/terralens"
exec_path="${destination}/terralens"
exec_path="${exec_path//\\/\\\\}"
exec_path="${exec_path//\"/\\\"}"
cat > "$app_dir/terralens.desktop" <<EOF_DESKTOP
[Desktop Entry]
Type=Application
Name=TerraLens
Comment=Native TerraFirmaGreg world atlas
Exec="$exec_path"
Icon=terralens
Terminal=false
Categories=Game;
Keywords=Minecraft;TerraFirmaGreg;TerraFirmaCraft;Prism;Seed;Map;
StartupWMClass=terralens
EOF_DESKTOP
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$app_dir"
fi
printf 'Installed TerraLens %s. Run: terralens\n' "$version"
INSTALL
chmod 0755 "$stage/install.sh"

# Build the corresponding-source archive from tracked and non-ignored untracked
# files in this exact checkout, including modifications. Ignored machine-local
# assets and build outputs stay out. Do not use git archive: it would omit edits.
python3 - "$repo_root" "$tmp_dir" "$package_name" "$source_overlay" <<'PY'
import hashlib, os, pathlib, subprocess, sys, tarfile
root = pathlib.Path(sys.argv[1]).resolve()
tmp = pathlib.Path(sys.argv[2])
name = sys.argv[3]
overlay = pathlib.Path(sys.argv[4])
raw = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z', '--cached', '--others', '--exclude-standard'])
paths = sorted({p.decode() for p in raw.split(b'\0') if p and not p.decode().startswith('dist/')})
entries = []
manifest_lines = []
base_manifest_lines = []
def add_source(path, rel):
    if not path.is_file() or path.is_symlink():
        return
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    manifest_lines.append(f'{digest}  {rel}\n')
    entries.append((path, rel))
for rel in paths:
    path = root / rel
    before = len(manifest_lines)
    add_source(path, rel)
    if len(manifest_lines) != before:
        base_manifest_lines.append(manifest_lines[-1])
for path in sorted(overlay.rglob('*')):
    if path.is_file() and not path.is_symlink():
        add_source(path, path.relative_to(overlay).as_posix())
expected = (tmp / 'source-before.sha256').read_text(encoding='utf-8')
if ''.join(base_manifest_lines) != expected:
    raise SystemExit('source tree changed while packaging; rebuild from a stable checkout and retry')
manifest = tmp / 'SOURCE-MANIFEST.sha256'
manifest.write_text(''.join(manifest_lines), encoding='utf-8')
archive = tmp / f'{name}-source.tar.gz'
with tarfile.open(archive, 'w:gz') as out:
    for path, rel in entries:
        out.add(path, arcname=f'{name}-source/{rel}', recursive=False)
    out.add(manifest, arcname=f'{name}-source/SOURCE-MANIFEST.sha256', recursive=False)
PY
install -m 0644 "$tmp_dir/SOURCE-MANIFEST.sha256" "$stage/SOURCE-MANIFEST.sha256"
source_archive="$output_dir/$package_name-source.tar.gz"
install -m 0644 "$tmp_dir/$package_name-source.tar.gz" "$source_archive"

tar -C "$tmp_dir" --sort=name --owner=0 --group=0 --numeric-owner -czf "$output_dir/$package_name.tar.gz" "$package_name"
(
  cd -- "$output_dir"
  sha256sum "$package_name.tar.gz" "$package_name-source.tar.gz" > "$package_name.sha256"
)
printf 'Created release package: %s\n' "$output_dir/$package_name.tar.gz"
printf 'Created corresponding source: %s\n' "$source_archive"
printf 'Created checksums: %s\n' "$output_dir/$package_name.sha256"
