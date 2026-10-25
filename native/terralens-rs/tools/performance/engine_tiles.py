#!/usr/bin/env python3
"""Benchmark actual release-engine tiles and compare complete pixel/biome/water payloads.

Uses only the standard library. Does not open a window, generate Minecraft chunks,
or write to a save. A fresh engine per case distinguishes cold work from layer reuse.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import struct
import subprocess
import time


def measure(binary, case, world, materials=False, display_overrides=None):
    name, tx, tz, zoom, detail, saved, dimension, shaded = case
    process = subprocess.Popen(
        [str(binary), "--engine"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, env={**os.environ, "RAYON_NUM_THREADS": "2"},
    )

    def send(value):
        data = json.dumps(value).encode()
        process.stdin.write(struct.pack("<II", len(data), 0) + data)
        process.stdin.flush()

    def exact(size):
        data = bytearray()
        deadline = time.monotonic() + 120
        while len(data) < size:
            if not select.select([process.stdout], [], [], max(0, deadline - time.monotonic()))[0]:
                raise TimeoutError(f"Engine stopped responding in {name}")
            part = os.read(process.stdout.fileno(), size - len(data))
            if not part:
                raise RuntimeError(process.stderr.read().decode())
            data.extend(part)
        return bytes(data)

    try:
        spec = {
            "seed": 0, "settings": {}, "epoch": 1, "dimension": dimension,
            "display": {"height_shading": shaded, "contours": {"enabled": shaded, "interval": 5}},
        }
        if display_overrides is not None:
            spec["display"].update(display_overrides)
        if materials:
            # The next tile samples the same procedural ground at the same scale.
            spec["display"]["grass_colors"] = True
            spec["display"]["grass_opacity"] = 1.0
        if saved:
            spec.update(world_path=str(world), source_revision=1)
        records = []
        layers = ["Biomes", "Ground"] if materials else ["Biomes", "Rainfall"]
        for layer in layers:
            key = {"x": tx, "z": tz, "zoom": zoom, "detail": detail, "layer": layer}
            start = time.monotonic()
            send({"Plan": {"epoch": 1, "wanted": [key], "tasks": [
                {"spec": spec, "key": key, "revision": 1},
            ]}})
            meta_size, bulk_size = struct.unpack("<II", exact(8))
            reply, bulk = json.loads(exact(meta_size)), exact(bulk_size)
            elapsed = time.monotonic() - start
            if "Tile" not in reply:
                raise RuntimeError(f"Unexpected tile reply: {reply}")
            send({"Complete": {"epoch": 1, "key": key}})
            record = {
                "case": name, "layer": layer, "seconds": round(elapsed, 6),
                "sha256": hashlib.sha256(bulk).hexdigest(), "bulk_bytes": len(bulk),
            }
            records.append(record)
            print(json.dumps(record), flush=True)
        send("Shutdown")
        process.wait(timeout=10)
        return records
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--world", type=Path, help="Optional seed-0 TFG world for saved-surface cases")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--compare", type=Path, help="Require identical tile payloads to this prior report")
    parser.add_argument("--extended", action="store_true", help="Also check unshaded and dimension views")
    parser.add_argument("--materials", action="store_true", help="Measure biome grass plus ground layer reuse")
    parser.add_argument("--materials-only", action="store_true", help="Run only the biome grass and ground layer workloads")
    args = parser.parse_args()
    earth = "minecraft:overworld"
    cases = [
        ("block-volcano-seed", 53, 55, 0, True, False, earth, True),
        ("block-volcano-save", 53, 55, 0, True, True, earth, True),
        ("block-water-save", 7, 4, 0, True, True, earth, True),
        ("medium-regional", 0, 0, 4, True, False, earth, True),
        ("far-overview", 0, 0, 8, False, False, earth, True),
        ("far-detail", 0, 0, 8, True, False, earth, True),
    ]
    if args.extended:
        cases += [
            ("unshaded-block", 53, 55, 0, True, False, earth, False),
            ("unshaded-medium", 0, 0, 4, True, False, earth, False),
            ("moon", 0, 0, 0, False, False, "ad_astra:moon", True),
            ("mars", 0, 0, 0, False, False, "ad_astra:mars", True),
            ("beneath", 0, 0, 0, False, False, "minecraft:the_nether", False),
        ]
    results = []
    if not args.materials_only:
        for case in cases:
            if case[5] and args.world is None:
                continue
            results.extend(measure(args.binary.resolve(), case, args.world))
    if args.materials or args.materials_only:
        for case in [
            ("seed-grass-ground-volcano", 53, 55, 0, True, False, earth, True),
            ("seed-grass-ground-medium", 0, 0, 4, True, False, earth, True),
        ]:
            results.extend(measure(args.binary.resolve(), case, args.world, materials=True))
    if args.compare:
        previous = json.loads(args.compare.read_text())
        if isinstance(previous, dict):
            previous = previous["results"]
        expected = {(row["case"], row["layer"]): row["sha256"] for row in previous}
        for row in results:
            key = (row["case"], row["layer"])
            if key not in expected or row["sha256"] != expected[key]:
                raise AssertionError(f"Tile payload differs from reference: {key}")
    args.output.write_text(json.dumps({
        "schema": 1, "seed": 0, "saved_world": args.world is not None,
        "scope": "Release child engine, cold tile then layer switch; complete RGBA/biome/water payload SHA-256; excludes native GPU presentation",
        "results": results,
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
