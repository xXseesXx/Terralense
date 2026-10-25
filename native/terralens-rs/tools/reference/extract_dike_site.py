#!/usr/bin/env python3
"""Read-only surface reference for the World 0 exposed-rock dike site.

Captures the nine FULL chunks surrounding the user's reported (-319262, 16107)
coordinate. This records saved natural substrate only; it does not launch
Minecraft or write to the world.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import extract_ground as ground
import extract_volcanic_surface_fluids as anvil

CENTER = (-319262, 16107)
WORLD_CHUNK = (-19954, 1006)
CHUNKS = [
    [cx, cz]
    for cz in range(WORLD_CHUNK[1] - 1, WORLD_CHUNK[1] + 2)
    for cx in range(WORLD_CHUNK[0] - 1, WORLD_CHUNK[0] + 2)
]


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def capture(world: pathlib.Path, output: pathlib.Path) -> None:
    region_paths = {
        world / "region" / f"r.{cx // 32}.{cz // 32}.mca"
        for cx, cz in CHUNKS
    }
    before = {path: sha(path) for path in region_paths}
    records = []
    for cx, cz in CHUNKS:
        root = anvil.chunk_nbt(world / "region", cx, cz)
        data = root.get("Level", root)
        if data.get("Status") not in {"minecraft:full", "full"}:
            raise RuntimeError(f"reference chunk is not FULL: {cx},{cz}")
        sections = [s for s in data.get("sections", []) if "block_states" in s]
        columns = ground.columns(sections)
        chunks = data.get("ForgeCaps", {}).get("tfc:chunk_data", {})
        palette = sorted({
            state["Name"]
            for section in sections
            for state in section["block_states"]["palette"]
        })
        rows = []
        for row in columns:
            index = row["z"] * 16 + row["x"]
            heights = chunks.get("surfaceHeight", [])
            rows.append({
                **row,
                "world_x": cx * 16 + row["x"],
                "world_z": cz * 16 + row["z"],
                "surface_height_forgecaps": heights[index] if len(heights) == 256 else None,
            })
        records.append({
            "chunk": [cx, cz],
            "status": data["Status"],
            "biome_palette": [
                biome if isinstance(biome, str) else biome.get("Name")
                for section in data.get("sections", [])
                for biome in section.get("biomes", {}).get("palette", [])
                if isinstance(biome, str) or isinstance(biome, dict) and "Name" in biome
            ],
            "block_palette": palette,
            "columns": rows,
        })
    after = {path: sha(path) for path in region_paths}
    if before != after:
        raise RuntimeError("save region changed during extraction; retry after it is stable")
    report = {
        "schema": 1,
        "provenance": {
            "kind": "independent_python_read_only_anvil_nbt",
            "world_alias": "0",
            "seed": 0,
            "dimension": "minecraft:overworld",
            "center": list(CENTER),
            "chunks": CHUNKS,
            "stable_during_extraction": True,
            "region_sha256": {path.name: digest for path, digest in before.items()},
            "notes": [
                "Nine complete saved chunks around the user's stone-lens report.",
                "Columns include highest natural substrate and surface height metadata.",
                "No Minecraft launch, chunk generation, or save writes.",
            ],
        },
        "chunks": records,
    }
    output.write_text(json.dumps(report, separators=(",", ":")) + "\n")
    rock_rows = [
        row for chunk in records for row in chunk["columns"]
        if any(family in row["ground_block"] for family in
               ("rock/raw/", "rock/hardened/", "rock/gravel/"))
    ]
    print(f"captured {len(CHUNKS)} chunks, {len(rock_rows)} exposed rock columns: {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("world", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[2] / "tests/fixtures/ground/dike-user-site-seed0.json")
    args = parser.parse_args()
    capture(args.world, args.output)
