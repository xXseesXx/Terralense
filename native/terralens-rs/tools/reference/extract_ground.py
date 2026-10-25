#!/usr/bin/env python3
"""Capture ground below vegetation in fixed FULL World 0 chunks, read-only.

The compact NBT fixtures contain only block-state palettes, packed arrays and chunk
status. Entities, containers, inventories and other save metadata are excluded.
"""
import argparse
import gzip
import hashlib
import json
import pathlib
import struct
import extract_volcanic_surface_fluids as anvil

SITES = [
    ("floodplain_nearshore", 552, 694),
    ("active_shield_canopy", -173, 282),
    ("cinder_cone", -79, 293),
    ("glacial_tuya", 3122, 1764),
    ("tuya_raw_rock", 344, -398),
    ("saved_plain", 0, 0),
    ("river_waterlogged_bank", 551, 694),
    ("glacial_snow_rim", 3123, 1765),
]
AIR = {"minecraft:air", "minecraft:cave_air", "minecraft:void_air"}
NATURAL = {"stone", "deepslate", "tuff", "dirt", "coarse_dirt", "grass_block",
           "podzol", "mycelium", "sand", "red_sand", "gravel", "snow_block", "ice",
           "packed_ice", "blue_ice", "sea_ice", "halite", "hardened_clay", "mud",
           "rooted_dirt", "clay", "magma_block", "water", "lava", "netherrack",
           "basalt", "blackstone", "end_stone", "pile/volcanic_ash", "farmland", "dirt_path"}
FAMILIES = ("grass/", "clay_grass/", "dirt/", "coarse_dirt/", "duff/", "clay/",
            "mud/", "rooted_dirt/", "sand/", "rock/raw/", "rock/hardened/",
            "rock/gravel/", "rock/cobble/", "rock/mossy_cobble/", "rock/magma/", "muddy_roots/", "farmland/", "grass_path/", "raw_sandstone/")
FLUIDS = {"water": "minecraft:water", "salt_water": "tfc:fluid/salt_water",
          "river_water": "tfc:fluid/river_water", "spring_water": "tfc:fluid/spring_water",
          "lava": "minecraft:lava"}


def substrate(state):
    name = state["Name"]
    path = name.split(":", 1)[-1]
    if path in NATURAL or path.startswith(FAMILIES) or path in {"fluid/salt_water", "fluid/river_water", "fluid/spring_water"}:
        return name
    props = state.get("Properties", {})
    fluid = next((props[k] for k in ("fluid", "fluid_type", "fluid_state") if k in props), None)
    if fluid is not None:
        return FLUIDS.get(fluid.split(":")[-1])
    if props.get("waterlogged") == "true":
        return "minecraft:water"
    return None


def columns(sections):
    # Decode independently of Rust, iterating complete section data bottom-up and
    # comparing absolute heights, including negative Y and unordered sections.
    tops, ground = [None] * 256, [None] * 256
    for section in sections:
        states = section["block_states"]
        palette = states["palette"]
        bits = max(4, (len(palette) - 1).bit_length())
        per_long = 64 // bits
        for index in range(4096):
            pi = 0 if len(palette) == 1 else (states["data"][index // per_long] >> ((index % per_long) * bits)) & ((1 << bits) - 1)
            state = palette[pi]
            name = state["Name"]
            if name in AIR:
                continue
            y = section["Y"] * 16 + index // 256
            col = index % 256
            if tops[col] is None or y > tops[col][0]:
                tops[col] = (y, name, state.get("Properties", {}))
            selected = substrate(state)
            if selected and (ground[col] is None or y > ground[col][0]):
                ground[col] = (y, selected)
    assert all(tops) and all(ground)
    return [{"x": i % 16, "z": i // 16, "top_y": top[0], "top_block": top[1],
             "top_properties": top[2], "ground_y": sub[0], "ground_block": sub[1]}
            for i, (top, sub) in enumerate(zip(tops, ground))]


def string(value):
    raw = value.encode("utf-8")
    return struct.pack(">H", len(raw)) + raw


def tag(kind, name, payload):
    return bytes([kind]) + string(name) + payload


def block_state(state):
    out = tag(8, "Name", string(state["Name"]))
    if state.get("Properties"):
        out += tag(10, "Properties", b"".join(tag(8, k, string(v)) for k, v in sorted(state["Properties"].items())) + b"\0")
    return out + b"\0"


def section_nbt(section):
    states = section["block_states"]
    palette = states["palette"]
    data = tag(9, "palette", b"\x0a" + struct.pack(">i", len(palette)) + b"".join(map(block_state, palette)))
    if "data" in states:
        longs = states["data"]
        data += tag(12, "data", struct.pack(">i", len(longs)) + b"".join(struct.pack(">q", n) for n in longs))
    return tag(1, "Y", struct.pack(">b", section["Y"])) + tag(10, "block_states", data + b"\0") + b"\0"


def capture(world, output):
    output.mkdir(parents=True, exist_ok=True)
    sites, hashes = [], {}
    for name, cx, cz in SITES:
        region = world / "region" / f"r.{cx // 32}.{cz // 32}.mca"
        digest = hashlib.sha256(region.read_bytes()).hexdigest()
        hashes.setdefault(region, digest)
        assert hashes[region] == digest, "save changed during extraction"
        root = anvil.chunk_nbt(world / "region", cx, cz)
        data = root.get("Level", root)
        assert data["Status"] in {"minecraft:full", "full"}, (cx, cz, data["Status"])
        sections = [s for s in data["sections"] if "block_states" in s]
        rows = columns(sections)
        # Preserve block state data; deliberately omit all other chunk data.
        nbt = tag(10, "", tag(8, "Status", string(data["Status"])) + tag(9, "sections", b"\x0a" + struct.pack(">i", len(sections)) + b"".join(map(section_nbt, sections))) + b"\0")
        filename = f"{name}.nbt.gz"
        (output / filename).write_bytes(gzip.compress(nbt, mtime=0))
        sites.append({"name": name, "chunk": [cx, cz], "region": region.name,
                      "region_sha256": digest, "nbt_fixture": filename, "columns": rows})
        print(f"{name}: {len(rows)} columns, {sum(r['top_block'] != r['ground_block'] for r in rows)} covered columns")
    for region, digest in hashes.items():
        assert hashlib.sha256(region.read_bytes()).hexdigest() == digest, "save changed during extraction"
    fixture = {"schema": 2, "provenance": {
        "kind": "independent_python_read_only_anvil_nbt", "world_alias": "0", "seed": 0,
        "dimension": "minecraft:overworld", "stable_during_extraction": True,
        "extraction_script": "tools/reference/extract_ground.py",
        "notes": ["All 256 columns per fixed FULL chunk, including vegetation and waterlogged cover.",
                  "NBT copies contain only original block-state sections and Status; no entities, containers or save metadata.",
                  "No Minecraft launch, chunk generation or save writes."]}, "sites": sites}
    (output / "seed0-ground-samples.json").write_text(json.dumps(fixture, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("world", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[2] / "tests/fixtures/ground")
    args = parser.parse_args()
    capture(args.world, args.output)
