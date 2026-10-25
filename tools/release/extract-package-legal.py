#!/usr/bin/env python3
"""Extract the already-verified shared Linux license bundle for the Windows ZIP."""
import argparse
from pathlib import Path, PurePosixPath
import tarfile


parser = argparse.ArgumentParser()
parser.add_argument("--archive", type=Path, required=True)
parser.add_argument("--package-name", required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
prefix = args.package_name + "/"
args.output.mkdir(parents=True, exist_ok=True)
copied = set()
with tarfile.open(args.archive, "r:gz") as archive:
    for member in archive:
        if not member.isfile() or not member.name.startswith(prefix):
            continue
        rel_text = member.name[len(prefix):]
        rel = PurePosixPath(rel_text)
        if rel.is_absolute() or ".." in rel.parts:
            raise SystemExit(f"unsafe package member path: {member.name}")
        if not (
            rel.parts[0] in {"licenses", "third-party-licenses", "provenance"}
            or rel_text in {"NOTICE.md", "THIRD-PARTY-NOTICES.md", "SOURCE-MANIFEST.sha256"}
        ):
            continue
        target = args.output.joinpath(*rel.parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        stream = archive.extractfile(member)
        if stream is None:
            raise SystemExit(f"cannot read package member: {member.name}")
        with target.open("wb") as output:
            while True:
                chunk = stream.read(1024 * 1024)
                if not chunk:
                    break
                output.write(chunk)
        copied.add(rel_text)
required = {
    "licenses/GPL-3.0.txt",
    "licenses/rust/provenance.json",
    "THIRD-PARTY-NOTICES.md",
    "SOURCE-MANIFEST.sha256",
}
missing = sorted(required - copied)
if missing:
    raise SystemExit(f"Linux package is missing shared legal/source files: {missing}")
print(f"Extracted {len(copied)} shared license, notice, provenance and manifest files")
