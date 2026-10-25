#!/usr/bin/env python3
"""Rebuild the bundled TFC/TFG tree-template metadata from the active Prism instance.

Only Python's standard library is required. KubeJS data has highest precedence,
followed by the installed TFG Core jar and AFC jar. Mod resources are extracted to
a temporary directory and removed automatically when this command exits.
"""
from __future__ import annotations

import argparse
import copy
import gzip
import json
import os
import struct
import tempfile
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = REPO / "src/vegetation/assets/tree_templates.json"
SOURCE = "Installed TFG Core 0.9.25, AFC 1.0.22, TFC 3.2.25 configured features and NBT structures"
TREE_VARIANTS = ("normal_tree", "dead_tree", "old_growth_tree", "krummholz")


class NbtReader:
    def __init__(self, data: bytes):
        self.data = data
        self.offset = 0

    def u8(self) -> int:
        value = self.data[self.offset]
        self.offset += 1
        return value

    def s16(self) -> int:
        value = struct.unpack_from(">h", self.data, self.offset)[0]
        self.offset += 2
        return value

    def s32(self) -> int:
        value = struct.unpack_from(">i", self.data, self.offset)[0]
        self.offset += 4
        return value

    def string(self) -> str:
        size = self.s16()
        value = self.data[self.offset : self.offset + size].decode("utf-8")
        self.offset += size
        return value

    def payload(self, tag: int):
        if tag == 0:
            return None
        if tag == 1:
            return struct.unpack(">b", bytes([self.u8()]))[0]
        if tag == 2:
            return self.s16()
        if tag == 3:
            return self.s32()
        if tag == 4:
            value = struct.unpack_from(">q", self.data, self.offset)[0]
            self.offset += 8
            return value
        if tag == 5:
            value = struct.unpack_from(">f", self.data, self.offset)[0]
            self.offset += 4
            return value
        if tag == 6:
            value = struct.unpack_from(">d", self.data, self.offset)[0]
            self.offset += 8
            return value
        if tag == 7:
            size = self.s32()
            value = self.data[self.offset : self.offset + size]
            self.offset += size
            return value
        if tag == 8:
            return self.string()
        if tag == 9:
            element_type, size = self.u8(), self.s32()
            return [self.payload(element_type) for _ in range(size)]
        if tag == 10:
            result = {}
            while True:
                element_type = self.u8()
                if element_type == 0:
                    return result
                key = self.string()
                result[key] = self.payload(element_type)
        if tag == 11:
            size = self.s32()
            values = list(struct.unpack_from(">" + "i" * size, self.data, self.offset))
            self.offset += 4 * size
            return values
        if tag == 12:
            size = self.s32()
            values = list(struct.unpack_from(">" + "q" * size, self.data, self.offset))
            self.offset += 8 * size
            return values
        raise ValueError(f"Unknown NBT tag type {tag}")

    def root(self):
        tag = self.u8()
        self.string()  # root name
        return self.payload(tag)


def load_nbt(path: Path):
    with gzip.open(path, "rb") as stream:
        data = stream.read()
    try:
        return NbtReader(data).root()
    except Exception as exc:
        raise ValueError(f"Invalid structure NBT {path}: {exc}") from exc


def is_tree_block(name: str) -> bool:
    return any(part in name for part in ("leaves", "wood/log/", "wood/branch/", "wood/stripped_", "wood/wood/"))


def tree_kind(name: str) -> int:
    return 0 if "leaves" in name else 1


def discover_instance(explicit: Path | None) -> Path:
    if explicit is not None:
        instance = explicit.expanduser().resolve()
        if not (instance / "minecraft").is_dir():
            raise SystemExit(f"Prism instance has no minecraft directory: {instance}")
        return instance

    data_home = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    instances = data_home / "PrismLauncher" / "instances"
    candidates = sorted(
        path for path in instances.iterdir()
        if path.is_dir() and (path / "minecraft/kubejs/data").is_dir()
        and "TerraFirmaGreg" in path.name
    ) if instances.is_dir() else []
    if not candidates:
        raise SystemExit("Could not find a TerraFirmaGreg Prism instance; pass --instance PATH")
    if len(candidates) > 1:
        preferred = next((path for path in candidates if path.name == "TerraFirmaGreg-Modern"), None)
        if preferred is None:
            raise SystemExit("Multiple TerraFirmaGreg Prism instances found; pass --instance PATH")
        return preferred
    return candidates[0]


def jar_with_marker(jars: list[Path], marker: str) -> Path:
    matches = []
    for jar in jars:
        try:
            with zipfile.ZipFile(jar) as archive:
                if marker in archive.namelist():
                    matches.append(jar)
        except zipfile.BadZipFile:
            continue
    if len(matches) != 1:
        raise SystemExit(f"Expected one installed jar containing {marker!r}, found {len(matches)}")
    return matches[0]


def extract_resource_data(jar: Path, destination: Path) -> Path:
    destination.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(jar) as archive:
        for name in archive.namelist():
            if not name.startswith("data/") or name.endswith("/"):
                continue
            parts = Path(name).parts
            if len(parts) < 3:
                continue
            if not ("worldgen/configured_feature" in name or "tags/worldgen/configured_feature" in name or "/structures/" in name):
                continue
            target = destination / Path(*parts[1:])
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(name))
    return destination


class DataSources:
    """Resource precedence: KubeJS > TFG Core > AFC."""
    def __init__(self, kube_data: Path, core_data: Path, afc_data: Path):
        self.high_to_low = [kube_data, core_data, afc_data]
        # Jar resources are layered: the TFG jar carries the base TFC tag and
        # TFG's additions, while AFC contributes to the same tag between them.
        # KubeJS is the highest-priority pack and is applied last.
        self.tag_low_to_high = [core_data, afc_data, kube_data]

    def find(self, namespace: str, section: str, relative: str) -> Path | None:
        for root in self.high_to_low:
            path = root / namespace / section / relative
            if path.is_file():
                return path
        return None

    def configured(self, feature_id: str):
        namespace, relative = feature_id.split(":", 1)
        path = self.find(namespace, "worldgen/configured_feature", relative + ".json")
        if path is None:
            return None
        return json.loads(path.read_text(encoding="utf-8"))

    def nbt(self, template_id: str):
        namespace, relative = template_id.split(":", 1) if ":" in template_id else ("tfc", template_id)
        path = self.find(namespace, "structures", relative + ".nbt")
        return load_nbt(path) if path else None

    def template(self, template_id: str):
        nbt = self.nbt(template_id)
        if not nbt:
            return None
        size = nbt["size"]
        center_x, center_z = (size[0] - 1) // 2, (size[2] - 1) // 2
        palettes = nbt.get("palettes") or [nbt.get("palette", [])]
        palette = palettes[0]
        top = {}
        for block in nbt.get("blocks", []):
            state, pos = block.get("state", -1), block.get("pos", [])
            if not isinstance(state, int) or not 0 <= state < len(palette) or len(pos) < 3:
                continue
            name = palette[state].get("Name", "")
            if not is_tree_block(name):
                continue
            x, z, y = int(pos[0]) - center_x, int(pos[2]) - center_z, int(pos[1])
            kind = tree_kind(name)
            old = top.get((x, z))
            if old is None or y > old[0] or (y == old[0] and kind == 1):
                top[x, z] = (y, kind)
        cells = [[x, z, y, kind] for (x, z), (y, kind) in sorted(top.items())]
        return {
            "id": template_id,
            "palette_count": len(palettes),
            "size_y": int(size[1]),
            "cells": cells,
            "footprint": [[x, z] for x, z, _, _ in cells],
        }

    def ordered_overlay_blocks(self, template_id: str):
        nbt = self.nbt(template_id)
        if not nbt:
            raise FileNotFoundError(f"No installed NBT structure for {template_id}")
        size = nbt["size"]
        center_x, center_z = (size[0] - 1) // 2, (size[2] - 1) // 2
        palette = (nbt.get("palettes") or [nbt.get("palette", [])])[0]
        full_blocks, other_blocks, block_entities = [], [], []
        for block in nbt.get("blocks", []):
            state, pos = block.get("state", -1), block.get("pos", [])
            if not isinstance(state, int) or not 0 <= state < len(palette) or len(pos) < 3:
                continue
            name = palette[state].get("Name", "")
            if name in ("minecraft:air", "minecraft:cave_air", "minecraft:void_air", "minecraft:structure_block", "minecraft:jigsaw"):
                continue
            kind = 0 if "leaves" in name else 1 if ("wood/log/" in name or "wood/branch/" in name or "palm_tree/trunk" in name) else 2
            point = [int(pos[0]) - center_x, int(pos[2]) - center_z, int(pos[1]), kind]
            if block.get("nbt") is not None:
                block_entities.append(point)
            elif kind == 1:
                full_blocks.append(point)
            else:
                other_blocks.append(point)
        # StructureTemplate.load partitions full, other, block-entity entries then
        # sorts each group by BlockPos.compareTo (Y, X, Z). This is processor order.
        key = lambda point: (point[2], point[0], point[1])
        return sorted(full_blocks, key=key) + sorted(other_blocks, key=key) + sorted(block_entities, key=key)

    def read_tag(self, tag_id: str, seen: set[str] | None = None):
        if seen is None:
            seen = set()
        if tag_id in seen:
            return []
        seen.add(tag_id)
        namespace, relative = tag_id.split(":", 1)
        result = []
        for root in self.tag_low_to_high:
            path = root / namespace / "tags/worldgen/configured_feature" / (relative + ".json")
            if not path.is_file():
                continue
            tag = json.loads(path.read_text(encoding="utf-8"))
            if tag.get("replace", False):
                result.clear()
            for value in tag.get("values", []):
                expanded = self.read_tag(value[1:], seen) if value.startswith("#") else [value]
                for item in expanded:
                    if item not in result:
                        result.append(item)
        return result


def tree_entry(config: dict):
    climate = config.get("climate", {})
    result = {key: config.get(key) for key in (*TREE_VARIANTS, "old_growth_chance", "spoiler_old_growth_chance", "dead_chance")}
    result["floating"] = bool(config.get("floating", False))
    result["climate"] = {
        key: climate[key]
        for key in ("min_temperature", "max_temperature", "min_rainfall", "max_rainfall")
        if key in climate
    }
    return result


def tree_trunk(config):
    trunk = config.get("trunk")
    if not trunk:
        return None
    return {
        "min_height": int(trunk.get("min_height", 0)),
        "max_height": int(trunk.get("max_height", 0)),
        "wide": bool(trunk.get("wide", False)),
        "block": trunk.get("state", {}).get("Name", ""),
    }


def root_config(config):
    if not config:
        return None
    return {
        "tries": int(config.get("tries", 5)),
        "width": int(config.get("width", 4)),
        "height": int(config.get("height", 3)),
        "required": bool(config.get("required", False)),
        "special_placer": config.get("special_placer"),
        "blocks": [
            {
                "replace": item.get("replace", []),
                "with": [block.get("block", block.get("Name", "")) for block in item.get("with", [])],
            }
            for item in config.get("blocks", [])
        ],
    }


def configured_tree(feature_id: str, configured: dict, sources: DataSources):
    feature_type = configured.get("type", "")
    config = configured.get("config", {})
    width = int(config.get("placement", {}).get("width", 1))
    height = int(config.get("placement", {}).get("height", 1))
    feature = {
        "width": width,
        "height": height,
        "ground_type": config.get("placement", {}).get("ground_type", "normal"),
        "trunk": tree_trunk(config),
        "root_system": root_config(config.get("root_system")),
        "variants": [],
    }
    if feature_type == "tfc:random_tree":
        feature["variants"] = [template for template_id in config.get("structures", []) if (template := sources.template(template_id)) and template["cells"]]
    elif feature_type in ("tfc:stacked_tree", "afc:trunkless_stacked_tree"):
        feature["feature_type"] = "stacked_tree" if feature_type == "tfc:stacked_tree" else "trunkless_stacked_tree"
        feature["layers"] = []
        if feature_type == "afc:trunkless_stacked_tree":
            feature["trunk"] = None
        for layer in config.get("layers", []):
            templates = [template for template_id in layer.get("templates", []) if (template := sources.template(template_id))]
            feature["layers"].append({
                "min_count": int(layer.get("min_count", 1)),
                "max_count": int(layer.get("max_count", 1)),
                "templates": templates,
            })
    elif feature_type == "tfc:overlay_tree":
        base = sources.template(config["base"])
        overlay = sources.template(config["overlay"])
        feature["feature_type"] = "overlay_tree"
        feature["overlay"] = {
            "base": base,
            "overlay": overlay,
            "integrity": float(config.get("overlay_integrity", 0.5)),
        } if base and overlay else None
    elif feature_type == "tfc:krummholz":
        provider = config.get("height", {}).get("value", {})
        feature["feature_type"] = "krummholz"
        feature["krummholz"] = {
            "block": config["block"],
            "min_height": int(provider.get("min_inclusive", 1)),
            "max_height": int(provider.get("max_inclusive", 1)),
        }
    else:
        raise ValueError(f"Unsupported referenced tree feature {feature_id}: {feature_type}")
    return feature


def build_asset(sources: DataSources):
    order = []
    entries = {}
    for feature_id in sources.read_tag("tfg:forest_trees"):
        configured = sources.configured(feature_id)
        if configured and configured.get("type") == "tfc:forest_entry" and feature_id not in entries:
            order.append(feature_id)
            entries[feature_id] = tree_entry(configured.get("config", {}))
    references = []
    for entry in entries.values():
        for key in TREE_VARIANTS:
            feature_id = entry.get(key)
            if feature_id and feature_id not in references:
                references.append(feature_id)
    features = {}
    for feature_id in references:
        configured = sources.configured(feature_id)
        if not configured:
            raise FileNotFoundError(f"Missing configured forest tree {feature_id}")
        features[feature_id] = configured_tree(feature_id, configured, sources)

    structures = {}
    for species, count in (("coconut", 4), ("date", 4), ("acai", 4), ("oil_palm", 3)):
        for index in range(count):
            structure_id = f"tfg:palm_tree/{species}_{index}"
            template = sources.template(structure_id)
            if not template:
                raise FileNotFoundError(f"Missing installed palm template {structure_id}")
            structures[structure_id] = template

    for feature in features.values():
        overlay = feature.get("overlay")
        if overlay:
            overlay["overlay"]["ordered_blocks"] = sources.ordered_overlay_blocks(overlay["overlay"]["id"])

    return {
        "schema": 3,
        "source": SOURCE,
        "feature_order": order,
        "entries": entries,
        "features": features,
        "structures": structures,
    }


def canonical(data):
    return json.dumps(data, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def comparison_view(data, reference):
    """Ignore only provenance and unused legacy random-tree height metadata.

    Layer heights affect stacked-tree placement and must compare strictly.
    Older random-tree variants omit size_y, which their placement never reads.
    """
    result = copy.deepcopy(data)
    result.pop("source", None)
    for feature_id, feature in result.get("features", {}).items():
        other = reference.get("features", {}).get(feature_id, {})
        if feature.get("feature_type", "random_tree") != "random_tree":
            continue
        if other.get("feature_type", "random_tree") != "random_tree":
            continue
        other_variants = {variant["id"]: variant for variant in other.get("variants", [])}
        for variant in feature.get("variants", []):
            previous = other_variants.get(variant["id"])
            if previous is not None and "size_y" not in previous:
                variant.pop("size_y", None)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--instance", type=Path, help="Prism instance directory (default: auto-detect TerraFirmaGreg)")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT, help=f"JSON asset path (default: {DEFAULT_OUTPUT})")
    parser.add_argument("--check", action="store_true", help="Compare regenerated metadata with output without writing")
    args = parser.parse_args()

    instance = discover_instance(args.instance)
    minecraft = instance / "minecraft"
    kube_data = minecraft / "kubejs/data"
    mods = minecraft / "mods"
    jars = sorted(mods.glob("*.jar"))
    core_jar = jar_with_marker(jars, "data/tfg/tags/worldgen/configured_feature/forest_trees.json")
    afc_jar = jar_with_marker(jars, "data/afc/tags/worldgen/configured_feature/normal_forest_trees.json")
    args.output = args.output.expanduser().resolve()

    with tempfile.TemporaryDirectory(prefix="terralens-tree-data-") as temporary:
        temp = Path(temporary)
        core_data = extract_resource_data(core_jar, temp / "core/data")
        afc_data = extract_resource_data(afc_jar, temp / "afc/data")
        asset = build_asset(DataSources(kube_data, core_data, afc_data))
        if args.check:
            existing = json.loads(args.output.read_text(encoding="utf-8"))
            same = canonical(asset) == canonical(existing)
            comparable = comparison_view(asset, existing)
            old_comparable = comparison_view(existing, asset)
            equivalent = canonical(comparable) == canonical(old_comparable)
            print(f"Generated {'matches' if same else 'matches semantically' if equivalent else 'DIFFERS from'} {args.output}")
            if not same:
                print(f"Generated counts: entries={len(asset['entries'])}, features={len(asset['features'])}, structures={len(asset['structures'])}")
                if equivalent:
                    print("Tree configs, ordered forest entries, template cells, overlay processor order,")
                    print("and structure data match, allowing only provenance and absent unused random-tree size_y.")
                    print("KubeJS configured-feature and structure resources were given precedence during extraction.")
                    return 0
                return 1
            print(f"Matched entries={len(asset['entries'])}, features={len(asset['features'])}, structures={len(asset['structures'])}")
            return 0

        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(asset, separators=(",", ":")), encoding="utf-8")
        print(f"Wrote {args.output}: entries={len(asset['entries'])}, features={len(asset['features'])}, structures={len(asset['structures'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
