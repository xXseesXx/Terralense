#!/usr/bin/env python3
"""Measure real child-engine Fast/Full tiles; optionally compare the previous app.

No native window, Minecraft process or save writes. Cold generation + second
layer timing is not a GPU frame-rate or desktop liveness measurement.
"""
import argparse
import json
from pathlib import Path
import engine_tiles

FAST = {name: False for name in (
    "precise_biome_boundaries", "physical_surface", "caves", "rivers_lakes",
    "volcanoes", "springs", "exposed_dikes", "large_rocks_special_soils", "surface_litter",
)}
FULL = {name: name != "surface_litter" for name in FAST}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--before", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    cases = [
        ("local-volcano", 53, 55, 0, True, False, "minecraft:overworld", False),
        ("medium-region", 0, 0, 4, True, False, "minecraft:overworld", False),
        ("continent-overview", 0, 0, 8, False, False, "minecraft:overworld", False),
    ]
    results = []
    profiles = [("fast", args.binary, FAST), ("full-terrain", args.binary, FULL)]
    if args.before:
        profiles.insert(0, ("previous", args.before, None))
    for profile, binary, terrain in profiles:
        for case in cases:
            overrides = {"height_shading": False}
            if terrain is not None:
                overrides["terrain"] = terrain
            records = engine_tiles.measure(binary.resolve(), case, None, display_overrides=overrides)
            for record in records:
                record["profile"] = profile
            results.extend(records)
    args.output.write_text(json.dumps({
        "scope": "Fresh release child engine per case, two workers; cold Biomes then Rainfall layer. Full terrain preserves source detail without visual trees/grass/litter; Fast skips terrain generators. Different water/biome precision is intentional. Excludes GPU and UI presentation.",
        "results": results,
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
