#!/usr/bin/env python3
"""Check a vendored source archive against the selected checkout and its manifest."""
import argparse
import hashlib
import pathlib
import subprocess
import tarfile


parser = argparse.ArgumentParser()
parser.add_argument("--repo", type=pathlib.Path, required=True)
parser.add_argument("--archive", type=pathlib.Path, required=True)
parser.add_argument("--manifest-out", type=pathlib.Path, required=True)
args = parser.parse_args()
root = args.repo.resolve()
archive_path = args.archive.resolve()

manifest_name = None
manifest_text = None
prefix = None
archive_hashes = {}
with tarfile.open(archive_path, "r|gz") as archive:
    for member in archive:
        if not member.isfile():
            continue
        if member.name.endswith("/SOURCE-MANIFEST.sha256"):
            if manifest_name is not None:
                raise SystemExit("source archive contains multiple source manifests")
            manifest_name = member.name
            prefix = manifest_name.removesuffix("/SOURCE-MANIFEST.sha256") + "/"
            stream = archive.extractfile(member)
            if stream is None:
                raise SystemExit("source manifest could not be read")
            manifest_text = stream.read().decode("utf-8")
            continue
        if prefix is None:
            if "/" not in member.name:
                raise SystemExit(f"source member is outside package root: {member.name}")
            prefix = member.name.split("/", 1)[0] + "/"
        if not member.name.startswith(prefix):
            raise SystemExit(f"source member is outside package root: {member.name}")
        rel = member.name[len(prefix):]
        stream = archive.extractfile(member)
        if stream is None:
            raise SystemExit(f"archive member is unreadable: {rel}")
        digest = hashlib.sha256()
        while True:
            chunk = stream.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
        if rel in archive_hashes:
            raise SystemExit(f"duplicate source archive member: {rel}")
        archive_hashes[rel] = digest.hexdigest()
if manifest_name is None or manifest_text is None or prefix is None:
    raise SystemExit("source archive does not contain a source manifest")

manifest = {}
for line_number, line in enumerate(manifest_text.splitlines(), 1):
    if not line:
        continue
    if len(line) < 67 or line[64:66] != "  ":
        raise SystemExit(f"invalid manifest entry on line {line_number}")
    digest, rel = line[:64], line[66:]
    if rel in manifest or len(digest) != 64:
        raise SystemExit(f"duplicate or invalid manifest path on line {line_number}: {rel}")
    manifest[rel] = digest

generated = {".cargo/config.toml"}
generated_prefix = "native/terralens-rs/vendor/"
raw = subprocess.check_output([
    "git", "-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard"
])
repo_paths = sorted({path.decode("utf-8") for path in raw.split(b"\0") if path and not path.decode("utf-8").startswith("dist/")})
repo_hashes = {}
for rel in repo_paths:
    source = root / pathlib.PurePosixPath(rel)
    if source.is_file() and not source.is_symlink():
        repo_hashes[rel] = hashlib.sha256(source.read_bytes()).hexdigest()

for rel, digest in repo_hashes.items():
    if manifest.get(rel) != digest:
        raise SystemExit(f"source snapshot does not match selected checkout: {rel}")
manifest_repo_paths = {rel for rel in manifest if rel not in generated and not rel.startswith(generated_prefix)}
if manifest_repo_paths != set(repo_hashes):
    missing = sorted(set(repo_hashes) - manifest_repo_paths)
    extra = sorted(manifest_repo_paths - set(repo_hashes))
    raise SystemExit(f"source path set differs; missing={missing[:5]}, extra={extra[:5]}")

for rel, digest in archive_hashes.items():
    expected = manifest.get(rel)
    if expected is None:
        raise SystemExit(f"archive contains file absent from source manifest: {rel}")
    if digest != expected:
        raise SystemExit(f"archive member hash differs from source manifest: {rel}")
if set(archive_hashes) != set(manifest):
    missing = sorted(set(manifest) - set(archive_hashes))
    extra = sorted(set(archive_hashes) - set(manifest))
    raise SystemExit(f"source archive file set differs; missing={missing[:5]}, extra={extra[:5]}")

args.manifest_out.parent.mkdir(parents=True, exist_ok=True)
args.manifest_out.write_text(manifest_text, encoding="utf-8", newline="\n")
print(f"Verified {len(repo_hashes)} checkout files and {len(archive_hashes) - len(repo_hashes)} vendored/generated source files")
