#!/usr/bin/env python3
"""Collect resolved Rust dependency license texts for a binary release bundle."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib


crate = Path(sys.argv[1]).resolve()
destination = Path(sys.argv[2]).resolve()
vendor_dir = Path(sys.argv[3]).resolve() if len(sys.argv) > 3 else None
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--format-version", "1",
    "--manifest-path", str(crate / "Cargo.toml"),
], text=True))

packages = sorted(
    (package for package in metadata["packages"] if Path(package["manifest_path"]).resolve() != (crate / "Cargo.toml").resolve()),
    key=lambda package: (package["name"].lower(), package["version"]),
)
vendor_packages = {}
if vendor_dir and vendor_dir.is_dir():
    for manifest in vendor_dir.glob("*/Cargo.toml"):
        try:
            package_meta = tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {})
            if package_meta.get("name") and package_meta.get("version"):
                vendor_packages[(package_meta["name"], package_meta["version"])] = manifest.parent
        except (OSError, tomllib.TOMLDecodeError):
            continue
repo_root = crate.parent.parent
provenance_file = repo_root / "licenses/rust/provenance.json"
provenance_packages = {}
if provenance_file.is_file():
    provenance_data = json.loads(provenance_file.read_text(encoding="utf-8"))
    provenance_packages = {
        (item["name"], item["version"]): item
        for item in provenance_data.get("packages", [])
    }
destination.mkdir(parents=True, exist_ok=True)
notice = [
    "# Third-party Rust dependency notices",
    "",
    "This release bundles the dependency license and notice files found in the locked Cargo packages below.",
    "Cargo.lock records the exact resolved versions. The full corresponding source archive includes",
    "vendored source for these packages.",
    "",
]
missing = []
copied = 0

def is_notice(filename: str) -> bool:
    base = filename.lower()
    return bool(re.match(r"^(licen[cs]e|copying|notice|copyright|ofl|ufl)([-_.].*)?$", base))

for package in packages:
    manifest = Path(package["manifest_path"])
    package_dir = vendor_packages.get((package["name"], package["version"]), manifest.parent)
    found = []
    explicit = package.get("license_file")
    if explicit:
        explicit_path = package_dir / explicit
        if explicit_path.is_file():
            found.append(explicit_path)
    if package_dir.exists():
        for current, dirs, files in os.walk(package_dir):
            relative = Path(current).relative_to(package_dir)
            dirs[:] = [d for d in dirs if d not in {".git", "target", "tests", "examples", "benches"}]
            if len(relative.parts) > 3:
                dirs[:] = []
            for filename in files:
                if is_notice(filename) or (relative.parts and relative.parts[0].lower() == "fonts" and filename.lower().endswith(".txt")):
                    found.append(Path(current) / filename)
    unique = sorted({item.resolve() for item in found if item.is_file()})
    license_expr = package.get("license") or "license metadata unavailable"
    fallback = provenance_packages.get((package["name"], package["version"]))
    notice.append(f"## {package['name']} {package['version']}")
    notice.append("")
    notice.append(f"Declared license: `{license_expr}`")
    if package.get("repository"):
        notice.append(f"Source: {package['repository']}")
    if unique or (fallback and fallback.get("notices")):
        target_dir = destination / f"{package['name']}-{package['version']}"
        paths = []
        target_dir.mkdir(parents=True, exist_ok=True)
        for source in unique:
            rel = source.relative_to(package_dir)
            target = target_dir / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
            paths.append(target.relative_to(destination).as_posix())
            copied += 1
        if fallback and fallback.get("notices"):
            upstream_dir = target_dir / "upstream"
            for source_item in fallback["notices"]:
                source = (repo_root / source_item["path"]).resolve()
                expected_hash = source_item.get("sha256")
                if not source.is_file():
                    raise SystemExit(f"missing upstream license notice: {source}")
                digest = hashlib.sha256(source.read_bytes()).hexdigest()
                if expected_hash and digest != expected_hash:
                    raise SystemExit(f"upstream license notice hash mismatch: {source}")
                target = upstream_dir / source.name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
                paths.append(target.relative_to(destination).as_posix())
                copied += 1
            notice.append(f"Upstream license/notice snapshot at commit `{fallback.get('commit', 'unknown')}`:")
        notice.extend(f"- `{path}`" for path in paths)
    else:
        missing.append((package["name"], package["version"], license_expr, package.get("repository")))
        notice.append("- No license/notice text file found in the Cargo package source; see the declared license and source above.")
    notice.append("")

(destination.parent / "THIRD-PARTY-NOTICES.md").write_text("\n".join(notice).rstrip() + "\n", encoding="utf-8")
print(f"Collected {copied} license/notice files for {len(packages)} locked Cargo packages")
if missing:
    print(f"Packages without an accompanying license/notice file: {len(missing)}")
    for name, version, license_expr, repository in missing:
        print(f"  {name} {version}: {license_expr}; {repository or 'no repository URL'}")
    raise SystemExit("Cannot package a release with missing dependency license notices")
