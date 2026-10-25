#!/usr/bin/env python3
"""Map TFC/TFG tree features to their actual leaf and trunk textures.

Run after assets/ground-colors/extract_palette.py. Reads installed feature/NBT data
and the generated Xaero texture samples; it writes only compact RGB values and IDs.
"""
from __future__ import annotations

import importlib.util
import gzip
import json
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
spec = importlib.util.spec_from_file_location("tree_templates", ROOT / "tools/extract_tree_templates.py")
tree_templates = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = tree_templates
spec.loader.exec_module(tree_templates)


def discover_blocks(feature, read_nbt):
    names: list[str] = []

    def visit(template_id):
        nbt = read_nbt(template_id)
        if not nbt:
            return
        palettes = nbt.get("palettes") or [nbt.get("palette", [])]
        for palette in palettes:
            for state in palette:
                name = state.get("Name", "")
                if any(token in name for token in ("wood/leaves/", "wood/log/", "wood/branch/", "wood/stripped_", "tfg:palm_tree/")):
                    names.append(name)

    def visit_config(config):
        if not isinstance(config, dict):
            return
        for template in config.get("variants", []):
            visit(template["id"])
        for layer in config.get("layers", []):
            for template in layer.get("templates", []):
                visit(template["id"])
        overlay = config.get("overlay")
        if overlay:
            for side in overlay.values():
                if isinstance(side, dict) and side.get("id"):
                    visit(side["id"])
        if config.get("krummholz", {}).get("block"):
            names.append(config["krummholz"]["block"])

    visit_config(feature)
    return names


def texture_alias(block: str, leaves: bool):
    if not block or ":" not in block:
        return None
    namespace, path = block.split(":", 1)
    if namespace == "tfg" and path.startswith("palm_tree/"):
        relative = path.removeprefix("palm_tree/").split("[", 1)[0]
        if relative == "trunk" and not leaves:
            # The size-specific TFG trunk models all use the TFC palm endgrain family.
            return "tfc:palm", "log_end"
        if relative.endswith("_leaves") and leaves:
            species = relative.removesuffix("_leaves")
            return f"tfg:palm_tree/{species}", "leaves"
    for kind in ("leaves", "log", "branch", "stripped_log", "stripped_branch", "stripped_wood", "wood"):
        prefix = f"wood/{kind}/"
        if path.startswith(prefix):
            species = path[len(prefix):].split("[", 1)[0]
            if not species:
                continue
            if leaves and kind != "leaves":
                continue
            if not leaves and kind == "leaves":
                continue
            if kind == "leaves":
                return f"{namespace}:{species}", "leaves"
            if kind in ("stripped_log", "stripped_wood"):
                return f"{namespace}:{species}", "stripped_log_end"
            if kind == "branch":
                return f"{namespace}:{species}", "branch"
            if kind == "stripped_branch":
                return f"{namespace}:{species}", "stripped_log_end"
            return f"{namespace}:{species}", "log_end"
    return None


def main():
    instance = tree_templates.discover_instance(None)
    minecraft = instance / "minecraft"
    jars = sorted((minecraft / "mods").glob("*.jar"))
    core_jar = tree_templates.jar_with_marker(jars, "data/tfg/tags/worldgen/configured_feature/forest_trees.json")
    afc_jar = tree_templates.jar_with_marker(jars, "data/afc/tags/worldgen/configured_feature/normal_forest_trees.json")
    archives = [zipfile.ZipFile(core_jar), zipfile.ZipFile(afc_jar)]
    tree_data = json.loads((ROOT / "src/vegetation/assets/tree_templates.json").read_text())
    texture_data = json.loads((ROOT / "assets/ground-colors/tree_palette.json").read_text())
    # Accept both the flat output of extract_palette.py and this script's
    # wrapped output, so rerunning this command alone remains idempotent.
    while isinstance(texture_data.get("textures"), dict):
        texture_data = texture_data["textures"]

    def read_nbt(template_id):
        namespace, relative = template_id.split(":", 1) if ":" in template_id else ("tfc", template_id)
        resource = f"data/{namespace}/structures/{relative}.nbt"
        local = minecraft / "kubejs/data" / namespace / "structures" / f"{relative}.nbt"
        if local.is_file():
            return tree_templates.NbtReader(gzip.decompress(local.read_bytes())).root()
        for archive in archives:
            if resource in archive.namelist():
                return tree_templates.NbtReader(gzip.decompress(archive.read(resource))).root()
        return None

    features = {}
    for feature_id, config in tree_data["features"].items():
        ids = discover_blocks(config, read_nbt)
        leaf_candidates = sorted({alias for block in ids if (alias := texture_alias(block, True))})
        wood_candidates = sorted({alias for block in ids if (alias := texture_alias(block, False))})
        features[feature_id] = {
            "leaves": next((key for key, kind in leaf_candidates if texture_data.get(key, {}).get(kind)), None),
            "wood": next((key for key, kind in wood_candidates if texture_data.get(key, {}).get(kind)), None),
        }

    # The TFG palm overlay is a separate configured structure family rather
    # than one of the TFC forest features. Read its NBT palette to bind the
    # exact fruit-type leaf texture and trunk material.
    for species in ("coconut", "date", "acai", "oil_palm"):
        for index in range(4 if species != "oil_palm" else 3):
            template_id = f"tfg:palm_tree/{species}_{index}"
            names = discover_blocks({"variants": [{"id": template_id}]}, read_nbt)
            leaf_candidates = sorted({alias for block in names if (alias := texture_alias(block, True))})
            wood_candidates = sorted({alias for block in names if (alias := texture_alias(block, False))})
            features[template_id] = {
                "leaves": next((key for key, kind in leaf_candidates if texture_data.get(key, {}).get(kind)), None),
                "wood": next((key for key, kind in wood_candidates if texture_data.get(key, {}).get(kind)), None),
            }

    for archive in archives:
        archive.close()
    result = {"schema": 1, "textures": texture_data, "features": features}
    output = ROOT / "assets/ground-colors/tree_palette.json"
    output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    missing = [key for key, value in features.items() if value["leaves"] is None and value["wood"] is None]
    print(f"Wrote {len(features)} exact tree feature aliases to {output}")
    print(f"Features without a textured leaf or log profile: {len(missing)}")
    if missing:
        print("\n".join(missing))


if __name__ == "__main__":
    main()
