#!/usr/bin/env python3
"""Bundle the active TFC/TFG tree survival tags as resolved block IDs.

Usage: python3 tools/reference/extract_tree_ground.py [PRISM_INSTANCE] [--check]
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import sys
import zipfile

REPO = Path(__file__).resolve().parents[2]
OUTPUT = REPO / "src/vegetation/assets/tree_ground.json"
VANILLA = {
    "minecraft:dirt": {
        "minecraft:dirt",
        "minecraft:coarse_dirt",
        "minecraft:rooted_dirt",
        "minecraft:podzol",
        "minecraft:mycelium",
        "minecraft:grass_block",
        "minecraft:muddy_mangrove_roots",
    },
    "minecraft:sand": {"minecraft:sand", "minecraft:red_sand"},
    # Minecraft 1.20.1 bamboo_plantable_on base closure, expanded here so
    # Core's additive tag entries resolve against the vanilla tag as installed.
    "minecraft:bamboo_plantable_on": {
        "minecraft:sand", "minecraft:red_sand", "minecraft:dirt",
        "minecraft:coarse_dirt", "minecraft:rooted_dirt", "minecraft:podzol",
        "minecraft:mycelium", "minecraft:grass_block",
        "minecraft:muddy_mangrove_roots", "minecraft:gravel",
        "minecraft:suspicious_gravel", "minecraft:bamboo",
        "minecraft:bamboo_sapling",
    },
}


def instance_path(arg: str | None) -> Path:
    if arg:
        path = Path(arg).expanduser().resolve()
    else:
        data = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
        path = data / "PrismLauncher/instances/TerraFirmaGreg-Modern"
    if not (path / "minecraft/mods").is_dir():
        raise SystemExit(f"No Prism mods folder at {path}")
    return path


def read_tags(path: Path) -> dict[str, dict]:
    # Loader tag contributions from all installed mods are additive unless a
    # contribution explicitly says replace=true. Include the Minecraft client
    # jar as well so vanilla tag closures are resolved from the exact version.
    jars = sorted((path / "minecraft/mods").glob("*.jar"))
    minecraft = Path.home() / ".local/share/PrismLauncher/instances/TerraFirmaGreg-Modern/minecraft/versions"
    jars.extend(sorted(minecraft.glob("*/.minecraft/client.jar")))
    result = {}
    for jar_path in jars:
        try:
            with zipfile.ZipFile(jar_path) as jar:
                for name in jar.namelist():
                    match = re.fullmatch(r"data/([^/]+)/tags/blocks/(.+)\.json", name)
                    if not match:
                        continue
                    namespace, tag = match.groups()
                    if namespace not in {"minecraft", "tfc", "tfg", "forge"}:
                        continue
                    key = f"{namespace}:{tag}"
                    incoming = json.loads(jar.read(name))
                    previous = result.get(key)
                    if previous is None or incoming.get("replace"):
                        result[key] = incoming
                    else:
                        previous["values"] = previous.get("values", []) + incoming.get("values", [])
        except (OSError, zipfile.BadZipFile):
            continue
    return result


def resolve(tag: str, files: dict[str, dict], seen: frozenset[str] = frozenset()) -> set[str]:
    if tag in seen:
        return set()
    entry = files.get(tag, {})
    result = set() if entry.get("replace") else set(VANILLA.get(tag, ()))
    nested_seen = seen | {tag}
    for value in entry.get("values", []):
        if value.startswith("#"):
            result.update(resolve(value[1:], files, nested_seen))
        else:
            result.add(value)
    return result


def apply_literal_kubejs_removals(path: Path, values: set[str]) -> set[str]:
    # This is the only literal pack-side removal from minecraft:dirt; the
    # original species block is outside the tree-support profile regardless.
    scripts = path / "minecraft/kubejs/server_scripts"
    if not scripts.is_dir():
        return values
    expression = re.compile(
        r"event\.remove\s*\(\s*(['\"])minecraft:dirt\1\s*,\s*(['\"])([^'\"]+)\2"
    )
    for script in scripts.rglob("*.js"):
        for match in expression.finditer(script.read_text(encoding="utf-8")):
            values.discard(match.group(3))
    return values


def main() -> int:
    args = [arg for arg in sys.argv[1:] if arg != "--check"]
    check = "--check" in sys.argv[1:]
    if len(args) > 1:
        raise SystemExit(__doc__)
    instance = instance_path(args[0] if args else None)
    tags = read_tags(instance)
    output = {
        "source": "Merged installed Prism mod and Minecraft block tags; KubeJS literal dirt removals",
        "tree_grows_on": sorted(resolve("tfc:tree_grows_on", tags)),
        "sand": sorted(resolve("minecraft:sand", tags)),
        "sea_bush_plantable_on": sorted(resolve("tfc:sea_bush_plantable_on", tags)),
        "bamboo_plantable_on": sorted(resolve("minecraft:bamboo_plantable_on", tags)),
    }
    output["tree_grows_on"] = sorted(
        apply_literal_kubejs_removals(instance, set(output["tree_grows_on"]))
    )
    rendered = json.dumps(output, indent=2) + "\n"
    if check:
        if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != rendered:
            raise SystemExit(f"Tree ground profile differs: {OUTPUT}")
        print("Tree support tags match the installed TFG profile")
    else:
        OUTPUT.write_text(rendered, encoding="utf-8")
        print(f"Wrote {OUTPUT}")
    print(json.dumps({key: len(value) for key, value in output.items() if isinstance(value, list)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
