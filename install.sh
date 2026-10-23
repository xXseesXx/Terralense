#!/usr/bin/env bash
set -euo pipefail
terralens_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
terralens_binary="$terralens_root/native/terralens-rs/target/release/terralens"
if [[ ! -x "$terralens_binary" ]]; then
  printf 'Build TerraLens first: %s/build.sh\n' "$terralens_root" >&2
  exit 1
fi
mkdir -p -- "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/scalable/apps"
ln -sfn -- "$terralens_binary" "$HOME/.local/bin/terralens"
cp -- "$terralens_root/assets/terralens.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/terralens.svg"
terralens_exec="${terralens_binary//\\/\\\\}"
terralens_exec="${terralens_exec//\"/\\\"}"
cat > "$HOME/.local/share/applications/terralens.desktop" <<EOF_DESKTOP
[Desktop Entry]
Type=Application
Name=TerraLens
Comment=Native Rust TerraFirmaGreg world atlas
Exec="$terralens_exec"
Icon=terralens
Terminal=false
Categories=Game;
Keywords=Minecraft;TerraFirmaGreg;TerraFirmaCraft;Prism;Seed;Map;
StartupWMClass=terralens
EOF_DESKTOP
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$HOME/.local/share/applications"
fi
printf 'Installed native TerraLens. Run: terralens\n'
