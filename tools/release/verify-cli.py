#!/usr/bin/env python3
"""Check production CLI metadata while safely capturing GUI-subsystem Windows output."""
import argparse
import json
import subprocess


parser = argparse.ArgumentParser()
parser.add_argument("executable")
parser.add_argument("--version", required=True)
parser.add_argument("--target", required=True)
args = parser.parse_args()

def run(*arguments):
    result = subprocess.run(
        [args.executable, *arguments],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
    )
    return result.stdout

version_text = run("--version")
if version_text.strip() != f"TerraLens {args.version}":
    raise SystemExit(f"unexpected version output: {version_text!r}")
help_text = run("--help")
if "Usage: terralens" not in help_text:
    raise SystemExit("production --help output did not contain the usage line")
info = json.loads(run("--build-info"))
expected = {
    "version": args.version,
    "target": args.target,
    "official": True,
    "diagnostics": False,
    "optional_game_artwork": False,
}
for key, value in expected.items():
    if info.get(key) != value:
        raise SystemExit(f"build-info {key}={info.get(key)!r}, expected {value!r}")
print(json.dumps(info, sort_keys=True))
