#!/usr/bin/env python3
"""Capture exact saved surface water and magma around seed-0 volcanic sites."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import pathlib
import time

ROOT = pathlib.Path(__file__).resolve().parents[4]
BASE = ROOT / "native/terralens-rs/tools/reference/extract_volcanic_surface_fluids.py"
SPEC = importlib.util.spec_from_file_location("surface_fluids", BASE)
surface = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(surface)

RADIUS = 1

# Fixed seed-0 saved centers supplied from prior investigations. Every column
# of every complete chunk in each 3x3 is captured; no feature/model filtering.
SITES = [
    ("water", 1856, 1147),
    ("water", 14840, 14327),
    ("water", -3191, 5149),
    ("magma_and_lava", 13664, 14159),
    ("magma_and_lava", -14048, 40773),
    ("magma_and_lava", -49483, 92583),
]
WATER_IDS = {
    "minecraft:water",
    "tfc:fluid/salt_water",
    "tfc:fluid/spring_water",
    "tfc:fluid/river_water",
}
MAGMA_PREFIX = "tfc:rock/magma/"


def source_files(world):
    paths = set()
    for _, x, z in SITES:
        for dz in range(-RADIUS, RADIUS + 1):
            for dx in range(-RADIUS, RADIUS + 1):
                cx, cz = x // 16 + dx, z // 16 + dz
                paths.add(world / "region" / f"r.{cx // 32}.{cz // 32}.mca")
    return sorted(paths)


def stamps(paths):
    values = {}
    for path in paths:
        if not path.is_file():
            values[path.name] = None
            continue
        stat = path.stat()
        values[path.name] = {"bytes": stat.st_size, "mtime_ns": stat.st_mtime_ns,
                             "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("world", type=pathlib.Path, help="Seed-0 Minecraft save folder (read-only)")
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    world = args.world.expanduser().resolve()
    files = source_files(world)
    before = stamps(files)
    rows = []
    for label, x, z in SITES:
        ccx, ccz = x // 16, z // 16
        columns, complete, missing, blocks, biomes = [], [], [], set(), set()
        for dz in range(-RADIUS, RADIUS + 1):
            for dx in range(-RADIUS, RADIUS + 1):
                cx, cz = ccx + dx, ccz + dz
                root = surface.chunk_nbt(world / "region", cx, cz)
                extracted = surface.block_columns(root, cx, cz) if root else None
                if extracted is None:
                    missing.append([cx, cz])
                    continue
                complete.append([cx, cz])
                columns.extend(extracted)
                blocks.update(col[3] for col in extracted)
                biomes.update(col[5] for col in extracted if col[5] is not None)
        if [ccx, ccz] not in complete:
            raise SystemExit(f"center chunk is not FULL: {label} at {x},{z}")
        rows.append({
            "role": label, "center": [x, z], "center_chunk": [ccx, ccz],
            "radius_chunks": RADIUS, "complete_chunks": sorted(complete),
            "missing_or_incomplete_chunks": sorted(missing),
            "block_palette": sorted(blocks), "biome_palette": sorted(biomes),
            "columns": columns,
        })
    after = stamps(files)
    if before != after:
        raise SystemExit("Save region files changed during extraction; discard and retry.")
    result = {
        "schema": 1,
        "provenance": {
            "kind": "read_only_generated_chunk_exact_surface_capture",
            "seed": 0, "world_alias": "0", "dimension": "minecraft:overworld",
            "chunk_radius": RADIUS,
            "sampling": "All 256 columns of every fully generated chunk in each fixed 3x3 saved-area sample; sites were chosen from previously identified water and cinder areas, and the sample is not conditioned on the model being evaluated.",
            "column_row": "[block_x, block_z, highest_non_air_y, exact_block_id, is_minecraft_lava, saved_surface_quart_biome]",
            "water_block_ids": sorted(WATER_IDS), "exposed_magma_prefix": MAGMA_PREFIX,
            "notes": [
                "Water and magma observations refer to the actual highest non-air block in each saved column.",
                "No model-based block selection, Minecraft launch, chunk generation, or save writes.",
            ],
            "stable_during_extraction": True,
            "region_sha256_before_after": before,
            "extraction_script": "native/terralens-rs/tools/reference/extract_volcanic_water_magma.py",
            "captured_unix_ns": time.time_ns(),
        },
        "sites": rows,
    }
    target = ROOT / "native/terralens-rs/tests/fixtures/volcanic-terrain/seed0-water-magma.json"
    if args.output is not None:
        target = args.output
    target.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    for site in rows:
        waters = [c for c in site["columns"] if c[3] in WATER_IDS]
        magma = [c for c in site["columns"] if c[3].startswith(MAGMA_PREFIX)]
        lava = [c for c in site["columns"] if c[4]]
        print(f"{site['role']} {site['center']}: full={len(site['complete_chunks'])} water={len(waters)} water_levels={sorted({c[2] for c in waters})} magma={len(magma)} magma_levels={sorted({c[2] for c in magma})} lava={len(lava)}")
        for kind, samples in (("water", waters), ("magma", magma)):
            for c in samples[:8]:
                print(f"  {kind}: ({c[0]},{c[1]}) y={c[2]} {c[3]}")


if __name__ == "__main__":
    main()
