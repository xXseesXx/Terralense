#!/usr/bin/env python3
"""Read installed litter configs, preserving source JSON and hashes for controls."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

IDS = ["tfc:sticks_shore_patch", "tfc:sticks_forest_patch", "tfc:salt_lick_patch",
       "tfg:earth/plant/dry_grass_patch", "tfg:earth/salt_lick_patch", "tfc:surface_loose_rocks"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("jar", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sources, features = {}, []
    with zipfile.ZipFile(args.jar) as archive:
        def read(ident, kind):
            namespace, path = ident.split(":")
            name = f"data/{namespace}/worldgen/{kind}/{path}.json"
            raw = archive.read(name)
            sources[name] = hashlib.sha256(raw).hexdigest()
            return json.loads(raw)
        for ident in IDS:
            placed = read(ident, "placed_feature")
            configured = read(placed["feature"], "configured_feature")
            child = read(configured["config"]["feature"], "placed_feature")
            features.append({"id": ident, "placed": placed, "configured": configured,
                             "child_placed": child})
    args.output.write_text(json.dumps({
        "provenance": "Direct ZIP resource decode from installed TFG Core; no Rust output used.",
        "jar_sha256": hashlib.sha256(args.jar.read_bytes()).hexdigest(),
        "resource_sha256": sources, "features": features,
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
