#!/usr/bin/env python3
"""Capture basal non-dike-host blocks from two saved World 0 chunks, read-only."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import extract_volcanic_surface_fluids as anvil


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("world", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    chunks = []
    for cx, cz in [(-7032, -972), (-19954, 1006)]:
        path = args.world / "region" / f"r.{cx//32}.{cz//32}.mca"
        before = hashlib.sha256(path.read_bytes()).hexdigest()
        root = anvil.chunk_nbt(args.world / "region", cx, cz)
        data = root.get("Level", root)
        assert data["Status"] in ("full", "minecraft:full")
        section = next(s for s in data["sections"] if s["Y"] == -4)
        states = section["block_states"]
        palette = [s["Name"] for s in states["palette"]]
        bits = max(4, (len(palette)-1).bit_length())
        per_long = 64 // bits

        def block(x, y, z):
            i = y*256 + z*16 + x
            index = 0 if len(palette) == 1 else (
                states["data"][i//per_long] >> ((i % per_long)*bits)
            ) & ((1 << bits)-1)
            return palette[index]

        counts = Counter()
        masks = []
        for x in range(16):
            for z in range(16):
                mask = 0
                for y in range(14):
                    name = block(x, y, z)
                    counts[name] += 1
                    if not name.startswith(("tfc:rock/raw/", "tfc:rock/hardened/", "tfc:rock/gravel/")):
                        mask |= 1 << y
                masks.append(mask)
        assert before == hashlib.sha256(path.read_bytes()).hexdigest()
        chunks.append({"chunk": [cx, cz], "region_sha256": before,
                       "non_host_masks_xz": masks, "observed_blocks": counts})
    args.output.write_text(json.dumps({
        "provenance": "Independent read-only Anvil decoder. World 0, 512 columns, Y -64..-51. X-then-Z bitmasks indicate observed non-host blocks. Saved final blocks validate exclusions, not every preceding feature state.",
        "chunks": chunks,
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
