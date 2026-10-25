#!/usr/bin/env python3
"""Read-only compact vertical reference profiles for the spread World 0 sample.

Uses the deterministic FULL-chunk coordinates in world0-spread-48. Captures a
4x4 block-column lattice through the near-surface volume and all 256 highest
natural surface columns. Snow/ice are marked excluded, never solid/air matches.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import extract_ground as ground_extractor
import extract_volcanic_surface_fluids as anvil

AIR = {"minecraft:air", "minecraft:cave_air", "minecraft:void_air"}
SNOW_ICE = {"minecraft:snow", "minecraft:snow_block", "minecraft:ice",
            "minecraft:packed_ice", "minecraft:blue_ice", "tfc:sea_ice"}


def classify(state: dict) -> str:
    name = state.get("Name", "minecraft:air")
    if name in SNOW_ICE:
        return "excluded_snow_ice"
    if name in AIR:
        return "air"
    path = name.split(":", 1)[-1]
    props = state.get("Properties", {})
    fluid = next((props[k] for k in ("fluid", "fluid_type", "fluid_state") if k in props), "")
    fluid = fluid.lower()
    if "lava" in path and "magma" not in path or "lava" in fluid:
        return "lava"
    if "water" in path or "water" in fluid or props.get("waterlogged") == "true":
        return "water"
    if path in {"snow", "snow_block", "ice", "packed_ice", "blue_ice", "sea_ice"}:
        return "excluded_snow_ice"
    return "solid"


def decode_sections(sections: list[dict]) -> tuple[list[dict[int, str]], set[int]]:
    cols: list[dict[int, str]] = [dict() for _ in range(256)]
    present_sections: set[int] = set()
    for section in sections:
        states = section.get("block_states", {})
        palette = states.get("palette", [])
        if not palette:
            continue
        sy = int(section["Y"])
        present_sections.add(sy)
        longs = states.get("data", [])
        bits = max(4, (len(palette) - 1).bit_length())
        per_long = 64 // bits
        mask = (1 << bits) - 1
        for index in range(4096):
            if len(palette) == 1:
                pi = 0
            else:
                word = longs[index // per_long] & ((1 << 64) - 1)
                pi = (word >> ((index % per_long) * bits)) & mask
            col = index % 256
            y = sy * 16 + index // 256
            cols[col][y] = classify(palette[pi])
    return cols, present_sections


def rle(values: list[tuple[int, str]]) -> list[list[int | str]]:
    runs: list[list[int | str]] = []
    for y, kind in values:
        if runs and runs[-1][2] == kind and runs[-1][0] + runs[-1][1] == y:
            runs[-1][1] += 1
        else:
            runs.append([y, 1, kind])
    return runs


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def capture(world: pathlib.Path, spread_path: pathlib.Path, output: pathlib.Path) -> None:
    spread = json.loads(gzip.decompress(spread_path.read_bytes()))
    chunks = []
    hashes: dict[str, str] = {}
    exclusions = {"snow_ice_voxels": 0, "unknown_voxels": 0}
    for record in spread["chunks"]:
        cx, cz = record["chunk"]
        region = world / "region" / f"r.{cx // 32}.{cz // 32}.mca"
        hashes.setdefault(region.name, digest(region))
        root = anvil.chunk_nbt(world / "region", cx, cz)
        data = root.get("Level", root)
        if data.get("Status") not in {"full", "minecraft:full"}:
            raise RuntimeError(f"reference chunk ceased to be FULL: {cx},{cz}")
        sections = data.get("sections", data.get("Sections", []))
        columns, section_ys = decode_sections(sections)
        caps = data.get("ForgeCaps", {}).get("tfc:chunk_data", {})
        noise = caps.get("surfaceHeight", [])
        if len(noise) != 256:
            raise RuntimeError(f"missing TFC surfaceHeight at {cx},{cz}")
        lattice = []
        low_y, high_limit = -32, 128
        for lz in (2, 6, 10, 14):
            for lx in (2, 6, 10, 14):
                col = lz * 16 + lx
                upper = min(int(noise[col]), high_limit)
                values = []
                for y in range(low_y, upper + 1):
                    kind = columns[col].get(y)
                    if kind is None:
                        kind = "unknown"
                        exclusions["unknown_voxels"] += 1
                    elif kind == "excluded_snow_ice":
                        exclusions["snow_ice_voxels"] += 1
                    values.append((y, kind))
                lattice.append({"local": [lx, lz], "y_min": low_y,
                                "y_max": upper, "rle": rle(values)})
        # Full 16x16 surface labels/coordinates allow exact surface-height and
        # material checks without pretending non-sampled interior voxels exist.
        ground_rows = ground_extractor.columns(sections)
        surface_rows = []
        visible_frozen_top_columns = 0
        for row, reference in zip(ground_rows,
                                  next(c["columns"] for c in spread["chunks"]
                                       if c["chunk"] == [cx, cz])):
            top = reference.get("surface_block", "")
            visible_frozen_top_columns += int(classify({"Name": top}) == "excluded_snow_ice")
            name = row["ground_block"]
            kind = classify({"Name": name})
            if kind == "excluded_snow_ice":
                exclusions["snow_ice_voxels"] += 1
            surface_rows.append([row["x"], row["z"], row["ground_y"], name, kind])
        chunks.append({
            "chunk": [cx, cz],
            "surface_height_forgecaps": [int(y) for y in noise],
            "forest_type_forgecaps": int(caps.get("forestType", 0)),
            "temperature_corners_forgecaps": caps.get("temperature", {}),
            "rainfall_corners_forgecaps": caps.get("rainfall", {}),
            "vertical_lattice_4x4": lattice,
            "highest_natural_surface_16x16": surface_rows,
            "visible_snow_ice_top_columns_informational": visible_frozen_top_columns,
        })
        print(f"profile {len(chunks)}/{len(spread['chunks'])}: {cx},{cz}", file=sys.stderr)

    for name, expected in hashes.items():
        path = world / "region" / name
        if digest(path) != expected:
            raise RuntimeError(f"source region changed during extraction: {name}")
    fixture = {
        "schema": 1,
        "provenance": {
            "source_fixture": spread_path.name,
            "source_ranking": spread["provenance"]["ranking_version"],
            "world_alias": "0", "seed": 0, "dimension": "minecraft:overworld",
            "read_only": True, "minecraft_launched": False,
            "region_sha256": dict(sorted(hashes.items())),
            "profile_y_range": "-32 through min(saved ForgeCaps surfaceHeight, 128), inclusive",
            "interior_sampling": "local x,z in {2,6,10,14} on each 16x16 chunk",
            "surface_sampling": "all 256 columns per sampled chunk; highest natural substrate",
            "voxel_classes": ["solid", "air", "water", "lava", "excluded_snow_ice", "unknown"],
            "exclusions": exclusions,
            "limitations": [
                "Profiles are per-voxel references at a deterministic 4x4 lattice, not a connected cave/ravine instance census.",
                "Columns touched by player/mod edits are not independently identified; classify surface material as natural-appearing only.",
                "Missing section data is unknown, never inferred as air.",
                "Already generated save chunks reflect player exploration bias.",
            ],
        },
        "chunks": chunks,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    encoded = json.dumps(fixture, separators=(",", ":"), sort_keys=True).encode()
    with output.open("wb") as stream:
        with gzip.GzipFile(filename="", fileobj=stream, mode="wb", mtime=0) as gz:
            gz.write(encoded)
    print(f"wrote {output} ({output.stat().st_size} bytes)", file=sys.stderr)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("world", type=pathlib.Path)
    parser.add_argument("--spread", type=pathlib.Path,
                        default=pathlib.Path(__file__).resolve().parents[2]
                        / "tests/fixtures/generator-accuracy/world0-spread-48.json.gz")
    parser.add_argument("--output", type=pathlib.Path,
                        default=pathlib.Path(__file__).resolve().parents[2]
                        / "tests/fixtures/generator-accuracy/world0-surface-profiles-48.json.gz")
    args = parser.parse_args()
    capture(args.world, args.spread, args.output)
