#!/usr/bin/env python3
"""Capture the fixed World 0 plateau surface controls without writing the save."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import extract_ground as ground


SITES = [
    ("rocky-plateau-rock-seed0-1550-704.json", 1550, 704),
    ("rocky-plateau-seed0-1536-704.json", 1536, 704),
]


def capture(world, output):
    output.mkdir(parents=True, exist_ok=True)
    for name, cx, cz in SITES:
        region = world / "region" / f"r.{cx // 32}.{cz // 32}.mca"
        digest = hashlib.sha256(region.read_bytes()).hexdigest()
        root = ground.anvil.chunk_nbt(world / "region", cx, cz)
        data = root.get("Level", root)
        assert data["Status"] in {"full", "minecraft:full"}
        rows = ground.columns([s for s in data["sections"] if "block_states" in s])
        biomes = sorted({b for s in data["sections"]
                         for b in s.get("biomes", {}).get("palette", [])})
        assert "tfg:earth/rocky_plateau" in biomes
        assert hashlib.sha256(region.read_bytes()).hexdigest() == digest, "save changed"
        fixture = {
            "schema": 1,
            "provenance": {
                "kind": "independent_python_read_only_anvil_nbt",
                "world_alias": "0", "seed": 0, "dimension": "minecraft:overworld",
                "chunk": [cx, cz], "region": region.name, "region_sha256": digest,
                "stable_during_extraction": True,
                "notes": [
                    "A fully generated saved chunk; its surface biome palette contains tfg:earth/rocky_plateau.",
                    "Ground is the highest natural substrate under vegetation, decoded independently from block-state palettes.",
                    "No Minecraft launch, chunk generation, or save writes.",
                ],
            },
            "biome_palette": biomes,
            "ground_blocks": dict(sorted(Counter(r["ground_block"] for r in rows).items())),
            "columns": rows,
        }
        (output / name).write_text(json.dumps(fixture, indent=2) + "\n")
        print(f"{cx},{cz}: {len(rows)} columns, {fixture['ground_blocks']}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("world", type=Path)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[2]
                        / "tests/fixtures/ground")
    args = parser.parse_args()
    capture(args.world, args.output)
