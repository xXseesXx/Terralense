#!/usr/bin/env python3
"""Capture a deterministic spread sample of already generated World 0 chunks.

No Minecraft process is started and the source save is opened read-only. Candidate
chunks are every allocated region slot, ranked by a versioned SHA-256 key; the
first N records whose status is FULL are retained. This is a reproducible sample
of the saved/generated footprint, not of unexplored world terrain.
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

VERSION = "terralens-world0-generated-chunk-rank-v1"
AIR = {"minecraft:air", "minecraft:cave_air", "minecraft:void_air"}


def state_name(state: dict) -> str:
    return state.get("Name", "minecraft:air")


def foliage_columns(sections: list[dict]) -> set[int]:
    """Columns with any leaf block, independent of the top-block sampler."""
    result: set[int] = set()
    for section in sections:
        section_y = section.get("Y")
        states = section.get("block_states", {})
        palette = states.get("palette", [])
        if not isinstance(section_y, int) or not palette:
            continue
        leaf_indices = {
            i for i, state in enumerate(palette)
            if ("/leaves/" in state_name(state) or state_name(state).endswith("_leaves")
                or state_name(state).endswith("_leaf"))
        }
        if not leaf_indices:
            continue
        if len(palette) == 1:
            result.update(range(256))
            continue
        longs = states.get("data")
        if longs is None:
            continue
        bits = max(4, (len(palette) - 1).bit_length())
        per_long = 64 // bits
        mask = (1 << bits) - 1
        for index in range(4096):
            word = longs[index // per_long] & ((1 << 64) - 1)
            palette_index = (word >> ((index % per_long) * bits)) & mask
            if palette_index in leaf_indices:
                result.add(index % 256)
    return result


def sample_chunk(root: dict, cx: int, cz: int, leaves: set[int]) -> dict | None:
    data = root.get("Level", root)
    if data.get("Status") not in ("full", "minecraft:full"):
        return None
    sections = data.get("sections", data.get("Sections", []))
    columns = ground_extractor.columns(sections)
    chunk_data = data.get("ForgeCaps", {}).get("tfc:chunk_data", {})
    surface_height = chunk_data.get("surfaceHeight")
    if not isinstance(surface_height, (tuple, list)) or len(surface_height) != 256:
        return None
    temperature = chunk_data.get("temperature", {})
    rainfall = chunk_data.get("rainfall", {})
    if not all(k in temperature and k in rainfall for k in ("00", "01", "10", "11")):
        return None
    output = []
    for row in columns:
        local_x, local_z = row["x"], row["z"]
        col = local_z * 16 + local_x
        biome = anvil.quart_surface_biome(
            data, sections, local_x, row["top_y"], local_z
        )
        output.append({
            "x": cx * 16 + local_x,
            "z": cz * 16 + local_z,
            "surface_y": row["top_y"],
            "surface_block": row["top_block"],
            "surface_properties": row["top_properties"],
            "ground_y": row["ground_y"],
            "ground_block": row["ground_block"],
            "terrain_noise_height": int(surface_height[col]),
            "surface_biome": biome,
            "has_leaf_block": col in leaves,
        })
    return {
        "chunk": [cx, cz],
        "forest_type": int(chunk_data.get("forestType", 0)),
        "temperature_corners": {k: float(temperature[k]) for k in ("00", "01", "10", "11")},
        "rainfall_corners": {k: float(rainfall[k]) for k in ("00", "01", "10", "11")},
        "columns": output,
    }


def digest(path: pathlib.Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for part in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(part)
    return value.hexdigest()


def saved_seed(level_dat: pathlib.Path) -> int:
    root = anvil.decode_nbt(gzip.decompress(level_dat.read_bytes()))
    return int(root["Data"]["WorldGenSettings"]["seed"])


def capture(world: pathlib.Path, output: pathlib.Path, count: int) -> None:
    region_dir = world / "region"
    candidates: list[tuple[bytes, int, int, pathlib.Path]] = []
    for path in sorted(region_dir.glob("r.*.*.mca")):
        parts = path.stem.split(".")
        if len(parts) != 3:
            continue
        rx, rz = int(parts[1]), int(parts[2])
        with path.open("rb") as stream:
            header = stream.read(4096)
        for index in range(1024):
            if header[index * 4:index * 4 + 4] == b"\0\0\0\0":
                continue
            cx, cz = rx * 32 + index % 32, rz * 32 + index // 32
            key = hashlib.sha256(f"{VERSION}:{cx}:{cz}".encode()).digest()
            candidates.append((key, cx, cz, path))
    candidates.sort()
    selected: list[dict] = []
    selected_regions: dict[str, str] = {}
    profile_root = world.parent.parent
    world_source_hashes = {
        name: digest(world / name)
        for name in ("data/tfg_worldgen_versions.dat",)
        if (world / name).is_file()
    }
    world_seed = saved_seed(world / "level.dat")
    if world_seed != 0:
        raise RuntimeError(f"expected World 0 seed 0, found {world_seed}")
    mod_jar = profile_root / "mods" / "TerraFirmaGreg-Core-Modern-0.9.25.jar"
    if mod_jar.is_file():
        world_source_hashes["installed_mod/TerraFirmaGreg-Core-Modern-0.9.25.jar"] = digest(mod_jar)
    visited = 0
    region_hash_before: dict[str, str] = {}
    for _, cx, cz, region_path in candidates:
        visited += 1
        if region_path.name not in region_hash_before:
            region_hash_before[region_path.name] = digest(region_path)
        root = anvil.chunk_nbt(region_dir, cx, cz)
        if not root:
            continue
        data = root.get("Level", root)
        sections = data.get("sections", data.get("Sections", []))
        sample = sample_chunk(root, cx, cz, foliage_columns(sections))
        if sample is None:
            continue
        selected.append(sample)
        selected_regions[region_path.name] = region_hash_before[region_path.name]
        print(f"{len(selected)}/{count}: chunk {cx},{cz}", file=sys.stderr)
        if len(selected) >= count:
            break
    if len(selected) < count:
        raise RuntimeError(f"only found {len(selected)} FULL TFC chunks; requested {count}")
    # Recheck fingerprints after capture so the reference is known stable.
    for name, expected in selected_regions.items():
        if digest(region_dir / name) != expected:
            raise RuntimeError(f"source region changed while extracting: {name}")
    for name, expected in world_source_hashes.items():
        path = (
            profile_root / "mods" / name.split("/", 1)[1]
            if name.startswith("installed_mod/")
            else world / name
        )
        if digest(path) != expected:
            raise RuntimeError(f"world profile changed while extracting: {name}")
    if saved_seed(world / "level.dat") != world_seed:
        raise RuntimeError("world seed changed while extracting")
    fixture = {
        "schema": 1,
        "provenance": {
            "method": "allocated region slots ranked by SHA-256(VERSION:chunkX:chunkZ), retaining first N FULL TFC chunks",
            "ranking_version": VERSION,
            "world_alias": "0",
            "seed": 0,
            "saved_level_seed": world_seed,
            "dimension": "minecraft:overworld",
            "read_only": True,
            "minecraft_launched": False,
            "candidate_allocated_slots": len(candidates),
            "candidates_visited_until_sample_complete": visited,
            "requested_full_chunks": count,
            "region_sha256": dict(sorted(selected_regions.items())),
            "world_source_sha256": dict(sorted(world_source_hashes.items())),
            "notes": [
                "This is a deterministic spread sample of already allocated/generated save chunks, not a sample of unexplored coordinates.",
                "Raw quart surface biome, TFC chunk climate corners, TFC surfaceHeight, highest saved block, substrate, and leaf presence are kept as distinct observations.",
                "Feature chunks include local player exploration bias; results are not a universal world-wide generation guarantee.",
            ],
        },
        "chunks": selected,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    encoded = json.dumps(fixture, separators=(",", ":"), sort_keys=True).encode()
    import gzip
    with output.open("wb") as stream:
        with gzip.GzipFile(filename="", fileobj=stream, mode="wb", mtime=0) as compressed:
            compressed.write(encoded)
    print(f"wrote {output} ({len(selected)} chunks, {output.stat().st_size} bytes)", file=sys.stderr)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("world", type=pathlib.Path)
    parser.add_argument("--chunks", type=int, default=48)
    parser.add_argument("--output", type=pathlib.Path,
                        default=pathlib.Path(__file__).resolve().parents[2]
                        / "tests/fixtures/generator-accuracy/world0-spread-48.json.gz")
    args = parser.parse_args()
    if not 1 <= args.chunks <= 256:
        parser.error("--chunks must be between 1 and 256")
    capture(args.world, args.output, args.chunks)
