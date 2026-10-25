#!/usr/bin/env python3
"""Capture saved ordinary-cinder lava tops and controls from seed 0, read only."""
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
# Positive sites were found by enumerating saved region coordinates against the
# exact TfgCellular2D cinder lattice and biome rarity thresholds, then retaining
# full saved chunks whose actual surface has lava. The dry/water cinder control
# is an eligible candidate; the active-shield control is an additional negative.
SITES = [
    ("positive_cinder", 13664, 14159),
    ("positive_cinder", -14048, 40773),
    ("positive_cinder", -49483, 92583),
    ("dry_cinder_water_control", -101364, -14703),
    ("dry_cinder_control", -1264, 4689),
]


def source_files(world):
    files = set()
    for _, x, z in SITES:
        ccx, ccz = x // 16, z // 16
        for dz in range(-RADIUS, RADIUS + 1):
            for dx in range(-RADIUS, RADIUS + 1):
                cx, cz = ccx + dx, ccz + dz
                files.add(world / "region" / f"r.{cx // 32}.{cz // 32}.mca")
    return sorted(files)


def stamps(paths):
    out = {}
    for path in paths:
        if not path.is_file():
            out[path.name] = None
            continue
        stat = path.stat()
        out[path.name] = {
            "bytes": stat.st_size,
            "mtime_ns": stat.st_mtime_ns,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("world", type=pathlib.Path, help="Seed-0 Minecraft save folder (read-only)")
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    world = args.world.expanduser().resolve()
    files = source_files(world)
    before = stamps(files)
    sites = []
    for family, x, z in SITES:
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
                for col in extracted:
                    columns.append(col)
                    blocks.add(col[3])
                    if col[5] is not None:
                        biomes.add(col[5])
        if [ccx, ccz] not in complete:
            raise SystemExit(f"center chunk is not fully generated: {family} {x},{z}")
        sites.append({
            "family": family, "center": [x, z], "center_chunk": [ccx, ccz],
            "radius_chunks": RADIUS, "complete_chunks": sorted(complete),
            "missing_or_incomplete_chunks": sorted(missing),
            "block_palette": sorted(blocks), "biome_palette": sorted(biomes),
            "columns": columns,
        })
    after = stamps(files)
    if before != after:
        raise SystemExit("Save region files changed during capture; discard and retry.")
    out = {
        "schema": 1,
        "provenance": {
            "kind": "read_only_generated_chunk_palette_and_surface_block_capture",
            "seed": 0, "dimension": "minecraft:overworld", "chunk_radius": RADIUS,
            "sampling": "All columns in every fully generated chunk of each bounded 3x3 sample; cinder candidate centers come from exact TfgCellular2D feature points and saved center-biome rarity thresholds.",
            "notes": [
                "Rows are [block_x, block_z, highest_non_air_y, block_id, exact_surface_lava, saved_quart_biome_at_surface].",
                "Surface lava is only true when the highest non-air saved block is minecraft:lava.",
                "Positive samples were selected by actual saved surface-lava presence to establish positive controls; complete chunks and all their columns are retained.",
                "The eligible dry cinder control has saved surface water in its sample; its family label describes the saved data role only and is not a claim that a marker or active shield exists.",
                "No Minecraft launch, chunk generation, or save writes.",
            ],
            "stable_during_extraction": True,
            "region_sha256_before_after": before,
            "extraction_script": "native/terralens-rs/tools/reference/extract_cinder_fluids.py",
            "captured_unix_ns": time.time_ns(),
        },
        "sites": sites,
    }
    target = ROOT / "native/terralens-rs/tests/fixtures/volcanic-terrain/seed0-cinder-fluids.json"
    if args.output is not None:
        target = args.output
    target.write_text(json.dumps(out, separators=(",", ":")) + "\n")
    for site in sites:
        lava = [c for c in site["columns"] if c[4]]
        water = sum(c[3] == "minecraft:water" for c in site["columns"])
        print(f"{site['family']} {site['center']}: full={len(site['complete_chunks'])} missing={len(site['missing_or_incomplete_chunks'])} columns={len(site['columns'])} lava={len(lava)} water={water} levels={sorted({c[2] for c in lava})}")


if __name__ == "__main__":
    main()
