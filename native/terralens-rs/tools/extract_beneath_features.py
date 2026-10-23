#!/usr/bin/env python3
"""Extract Beneath vein and structure rules from an installed TFG Prism instance.

The runtime data remains embedded JSON. This script is a developer tool for refreshing
that asset after a pack update. No game process or Minecraft save is opened or changed.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
import zipfile
from collections import defaultdict
from typing import Any


REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_INSTANCE = pathlib.Path.home() / ".local/share/PrismLauncher/instances/TerraFirmaGreg-Modern"
DEFAULT_OUTPUT = REPO_ROOT / "src/beneath/ore-features.json"
BIOME_TAG_ROOT = "tags/worldgen/biome/"


def fail(message: str) -> "NoReturn":
    raise SystemExit(message)


def minecraft_dir(instance: pathlib.Path) -> pathlib.Path:
    """Accept either a Prism instance root or its `minecraft` directory."""
    instance = instance.expanduser().resolve()
    if (instance / "kubejs").is_dir() and (instance / "mods").is_dir():
        return instance
    game = instance / "minecraft"
    if (game / "kubejs").is_dir() and (game / "mods").is_dir():
        return game
    fail(f"Not a Prism instance or minecraft directory: {instance}")


def locate_core_jar(mods: pathlib.Path) -> pathlib.Path:
    candidates = sorted(mods.glob("TerraFirmaGreg-Core-Modern-*.jar"))
    if len(candidates) != 1:
        names = ", ".join(path.name for path in candidates) or "none found"
        fail(f"Expected one TerraFirmaGreg Core jar in {mods}; found: {names}")
    return candidates[0]


def tag_id_from_resource(resource: str) -> str | None:
    """Convert data/<namespace>/tags/worldgen/biome/<path>.json to namespace:path."""
    parts = pathlib.PurePosixPath(resource).parts
    if len(parts) < 5 or parts[0] != "data" or parts[2] != "tags":
        return None
    if parts[3:5] != ("worldgen", "biome") or not resource.endswith(".json"):
        return None
    tag_path = pathlib.PurePosixPath(*parts[5:]).with_suffix("").as_posix()
    if not tag_path:
        return None
    return f"{parts[1]}:{tag_path}"


def load_tag_documents(core_jar: pathlib.Path, kubejs_data: pathlib.Path) -> dict[str, list[dict[str, Any]]]:
    """Read jar biome tags and overlay KubeJS tag definitions with pack semantics.

    A tag JSON with `replace: true` discards lower-priority values. Otherwise its values
    append to the lower-priority definition. Duplicate values are retained here, as in
    Minecraft's tag construction; the resolved output is de-duplicated in encounter order.
    """
    docs: dict[str, list[dict[str, Any]]] = defaultdict(list)
    try:
        with zipfile.ZipFile(core_jar) as jar:
            for resource in sorted(jar.namelist()):
                tag_id = tag_id_from_resource(resource)
                if tag_id is not None:
                    docs[tag_id].append(json.loads(jar.read(resource)))
    except (OSError, zipfile.BadZipFile, json.JSONDecodeError) as exc:
        fail(f"Could not read biome tags from {core_jar.name}: {exc}")

    for resource in sorted(kubejs_data.glob("*/tags/worldgen/biome/**/*.json")):
        relative = resource.relative_to(kubejs_data.parent).as_posix()
        tag_id = tag_id_from_resource(relative)
        if tag_id is not None:
            try:
                docs[tag_id].append(json.loads(resource.read_text(encoding="utf-8")))
            except (OSError, json.JSONDecodeError) as exc:
                fail(f"Could not read KubeJS biome tag {resource}: {exc}")

    merged: dict[str, list[Any]] = {}
    for tag_id, definitions in docs.items():
        values: list[Any] = []
        for definition in definitions:
            incoming = definition.get("values", [])
            if not isinstance(incoming, list):
                fail(f"Biome tag {tag_id} has a non-list `values` field")
            if definition.get("replace") is True:
                values.clear()
            values.extend(incoming)
        merged[tag_id] = values
    return merged


def resolve_tags(raw: dict[str, list[Any]]) -> dict[str, list[str]]:
    """Resolve nested `#namespace:tag` references and reject cycles or missing tags."""
    resolved: dict[str, list[str]] = {}

    def resolve(tag_id: str, stack: tuple[str, ...]) -> list[str]:
        if tag_id in resolved:
            return resolved[tag_id]
        if tag_id in stack:
            fail("Biome tag cycle: " + " -> ".join((*stack, tag_id)))
        if tag_id not in raw:
            fail(f"Missing biome tag referenced by installed configuration: {tag_id}")
        out: list[str] = []
        for entry in raw[tag_id]:
            required = True
            if isinstance(entry, dict):
                required = entry.get("required", True)
                entry = entry.get("id")
            if not isinstance(entry, str):
                fail(f"Biome tag {tag_id} contains an invalid entry")
            if entry.startswith("#"):
                reference = entry[1:]
                if reference not in raw and not required:
                    continue
                out.extend(resolve(reference, (*stack, tag_id)))
            else:
                out.append(entry)
        # Tag membership is set-like; retain deterministic source order.
        resolved[tag_id] = list(dict.fromkeys(out))
        return resolved[tag_id]

    for tag_id in sorted(raw):
        resolve(tag_id, ())
    return resolved


def extract_veins(kubejs_data: pathlib.Path) -> dict[str, dict[str, Any]]:
    feature_root = kubejs_data / "tfg/worldgen/configured_feature/nether/vein"
    if not feature_root.is_dir():
        fail(f"Beneath vein configured features not found: {feature_root}")
    veins: dict[str, dict[str, Any]] = {}
    for path in sorted(feature_root.glob("*.json")):
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            fail(f"Could not read configured feature {path}: {exc}")
        config = doc.get("config", {})
        shape = doc.get("type", "").split(":")[-1].replace("_vein", "")
        if shape not in {"cluster", "pipe", "disc"}:
            continue
        blocks: dict[str, list[Any]] = {}
        for entry in config.get("blocks", []):
            replacements = entry.get("replace", [])
            outputs = entry.get("with", [])
            if not isinstance(replacements, list) or not isinstance(outputs, list):
                fail(f"Invalid block replacement list in {path}")
            for host in replacements:
                blocks.setdefault(host, []).extend(outputs)
        normalized = {
            key: config[key]
            for key in (
                "random_name", "rarity", "size", "height", "radius", "min_y", "max_y",
                "min_skew", "max_skew", "min_slant", "max_slant", "sign", "density",
                "project", "project_offset", "near_lava", "biomes",
            )
            if key in config
        }
        normalized.update(type=shape, blocks=blocks)
        veins[path.stem] = normalized
    if len(veins) != 29:
        fail(f"Expected 29 Beneath ore features, found {len(veins)} in {feature_root}")
    return veins


def extract_structures(kubejs_data: pathlib.Path) -> list[dict[str, Any]]:
    tags_script = kubejs_data.parent / "server_scripts/tfg/beneath/tags.beneath.js"
    if not tags_script.is_file():
        fail(f"Beneath structure biome tag script not found: {tags_script}")
    script_text = tags_script.read_text(encoding="utf-8")
    members = re.findall(
        r"event\.add\(['\"]tfg:nether_biomes['\"],\s*['\"]([^'\"]+)['\"]\)",
        script_text,
    )
    if len(members) != 11 or len(set(members)) != 11:
        fail(f"Expected 11 unique KubeJS nether biome members, found {len(set(members))}")
    base = kubejs_data / "tfg/worldgen"
    structures = []
    for name in ("tower", "outpost"):
        try:
            structure_set = json.loads((base / f"structure_set/beneath/{name}.json").read_text(encoding="utf-8"))
            structure = json.loads((base / f"structure/beneath/{name}.json").read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            fail(f"Could not load Beneath {name} structure data: {exc}")
        biome_gate = structure.get("biomes") or structure.get("____biomes")
        if biome_gate != "#tfg:has_structure/tower":
            fail(f"Unexpected {name} biome gate: {biome_gate!r}")
        placement = structure_set["placement"]
        structures.append(
            {
                "id": f"tfg:beneath/{name}",
                "salt": placement["salt"],
                "spacing": placement["spacing"],
                "separation": placement["separation"],
                "biomes": members,
                "exclusion_zone": placement.get("exclusion_zone"),
                "start_y": structure["start_height"]["absolute"],
            }
        )
    return structures


def semantic(value: Any) -> Any:
    """Ignore provenance only; all generated behavior remains strict in --check."""
    if isinstance(value, dict):
        return {key: semantic(item) for key, item in value.items() if key != "_meta"}
    if isinstance(value, list):
        return [semantic(item) for item in value]
    return value


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--instance",
        type=pathlib.Path,
        default=DEFAULT_INSTANCE,
        help="Prism instance root or its minecraft directory (default: standard TFG Prism instance)",
    )
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        default=DEFAULT_OUTPUT,
        help=f"JSON asset to write or compare (default: {DEFAULT_OUTPUT.relative_to(REPO_ROOT)})",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare extracted behavior with --output without writing any files",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    game = minecraft_dir(args.instance)
    core_jar = locate_core_jar(game / "mods")
    kubejs_data = game / "kubejs/data"
    raw_tags = load_tag_documents(core_jar, kubejs_data)
    tags = resolve_tags(raw_tags)
    veins = extract_veins(kubejs_data)
    referenced_tags = {
        value[1:]
        for vein in veins.values()
        for value in [vein.get("biomes", "")]
        if isinstance(value, str) and value.startswith("#")
    }
    missing = sorted(referenced_tags - tags.keys())
    if missing:
        fail(f"Missing installed biome tag definitions: {', '.join(missing)}")
    # Keep every Beneath biome tag, not only those currently used as vein gates. The
    # garnet/oilsands tags are also used by terrain features and are part of the atlas data.
    relevant_tags = {tag: tags[tag] for tag in sorted(tags) if tag.startswith("tfg:nether/")}
    structures = extract_structures(game / "kubejs/data")
    output = {
        "_meta": {
            "source": "installed TFG Core jar biome tags + Prism KubeJS data",
            "core_jar": core_jar.name,
            "vein_count": len(veins),
            "biome_tag_count": len(relevant_tags),
        },
        "veins": veins,
        "biome_tags": relevant_tags,
        "structures": structures,
    }
    target = args.output.expanduser().resolve()
    if args.check:
        try:
            current = json.loads(target.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            fail(f"Cannot compare asset {target}: {exc}")
        if semantic(current) != semantic(output):
            fail(f"Beneath feature asset is stale: {target}\nRun without --check to refresh it.")
        print(
            f"Beneath asset matches: {len(veins)} veins, {len(relevant_tags)} biome tags, "
            f"{len(structures)} structures"
        )
        return 0

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(output, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        f"Wrote {len(veins)} veins, {len(relevant_tags)} biome tags and "
        f"{len(structures)} structures to {target}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
