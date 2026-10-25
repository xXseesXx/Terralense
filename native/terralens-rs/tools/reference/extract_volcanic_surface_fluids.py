#!/usr/bin/env python3
"""Capture actual lava and terrain in saved TFG volcanic chunks, without Minecraft."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import pathlib
import struct
import time
import zlib

_NBT_CONTEXT = []

SITES = [
    ("cinder_cone", -1264, 4689),
    ("active_shield", -2754, 4513),
    ("tuya", 5518, -6365),
    ("extinct_shield", 19624, -3052),
    ("glacial_tuya", 49954, 28225),
]


def _read_exact(stream: io.BytesIO, length: int) -> bytes:
    value = stream.read(length)
    if len(value) != length:
        raise ValueError("truncated NBT")
    return value


def _string(stream: io.BytesIO) -> str:
    size = struct.unpack(">H", _read_exact(stream, 2))[0]
    return _read_exact(stream, size).decode("utf-8", errors="replace")


def _payload(stream: io.BytesIO, kind: int):
    if kind == 1:
        return struct.unpack(">b", _read_exact(stream, 1))[0]
    if kind == 2:
        return struct.unpack(">h", _read_exact(stream, 2))[0]
    if kind == 3:
        return struct.unpack(">i", _read_exact(stream, 4))[0]
    if kind == 4:
        return struct.unpack(">q", _read_exact(stream, 8))[0]
    if kind == 5:
        return struct.unpack(">f", _read_exact(stream, 4))[0]
    if kind == 6:
        return struct.unpack(">d", _read_exact(stream, 8))[0]
    if kind == 7:
        size = struct.unpack(">i", _read_exact(stream, 4))[0]
        if size < 0:
            raise ValueError("negative byte array length")
        return _read_exact(stream, size)
    if kind == 8:
        return _string(stream)
    if kind == 9:
        child, size = struct.unpack(">Bi", _read_exact(stream, 5))
        if size < 0:
            raise ValueError("negative NBT list length")
        return [_payload(stream, child) for _ in range(size)]
    if kind == 10:
        compound = {}
        while True:
            child = _read_exact(stream, 1)[0]
            if child == 0:
                return compound
            name = _string(stream)
            _NBT_CONTEXT.append(name)
            try:
                compound[name] = _payload(stream, child)
            finally:
                _NBT_CONTEXT.pop()
    if kind == 11:
        size = struct.unpack(">i", _read_exact(stream, 4))[0]
        if size < 0:
            raise ValueError("negative int array length")
        return struct.unpack(f">{size}i", _read_exact(stream, size * 4))
    if kind == 12:
        size = struct.unpack(">i", _read_exact(stream, 4))[0]
        if size < 0:
            raise ValueError("negative long array length")
        return struct.unpack(f">{size}q", _read_exact(stream, size * 8))
    raise ValueError(f"unknown NBT tag {kind} at byte {stream.tell()} path={_NBT_CONTEXT[-10:]}")


def decode_nbt(raw: bytes):
    stream = io.BytesIO(raw)
    root_kind = _read_exact(stream, 1)[0]
    if root_kind != 10:
        raise ValueError("expected NBT compound")
    _string(stream)
    return _payload(stream, root_kind)


def chunk_nbt(region_dir: pathlib.Path, cx: int, cz: int):
    rx, rz = cx // 32, cz // 32
    path = region_dir / f"r.{rx}.{rz}.mca"
    if not path.is_file():
        return None
    local_x, local_z = cx % 32, cz % 32
    index = local_x + local_z * 32
    with path.open("rb") as file:
        file.seek(index * 4)
        entry = file.read(4)
        if len(entry) != 4 or entry == b"\0\0\0\0":
            return None
        sector = int.from_bytes(entry[:3], "big")
        if sector == 0:
            return None
        file.seek(sector * 4096)
        length_raw = file.read(4)
        if len(length_raw) != 4:
            return None
        length = int.from_bytes(length_raw, "big")
        if length < 2 or length > 64 * 1024 * 1024:
            return None
        # The length counts bytes after the length word, including compression ID.
        compressed = file.read(length)
    compression = compressed[0]
    payload = compressed[1:]
    try:
        if compression == 2:
            raw = zlib.decompress(payload)
        elif compression == 1:
            raw = zlib.decompress(payload, 16 + zlib.MAX_WBITS)
        elif compression == 3:
            raw = payload
        else:
            return None
        return decode_nbt(raw)
    except (zlib.error, ValueError, struct.error):
        # Treat an incomplete or corrupt location record as unavailable; never infer
        # generated blocks from a chunk status or a model estimate.
        return None


def palette_names(entries):
    return [entry.get("Name", "minecraft:air") for entry in entries]


def quart_surface_biome(data, sections, local_x, surface_y, local_z):
    quart_y = surface_y // 4
    section_y = quart_y // 4
    qy = quart_y % 4
    qx, qz = local_x // 4, local_z // 4
    for section in sections:
        if section.get("Y") != section_y:
            continue
        biomes = section.get("biomes", {})
        names = biomes.get("palette", [])
        if len(names) == 1:
            return names[0]
        longs = biomes.get("data")
        if not longs:
            return None
        bits = max(1, (len(names) - 1).bit_length())
        per_long = 64 // bits
        index = qx + qz * 4 + qy * 16
        packed = longs[index // per_long] & ((1 << 64) - 1)
        palette_index = (packed >> ((index % per_long) * bits)) & ((1 << bits) - 1)
        return names[palette_index] if palette_index < len(names) else None
    return None


def block_columns(root, cx, cz):
    data = root.get("Level", root)
    status = data.get("Status")
    if status not in ("full", "minecraft:full"):
        return None
    sections = data.get("sections", data.get("Sections", []))
    best = [None] * 256
    for section in sections:
        section_y = section.get("Y")
        if not isinstance(section_y, int):
            continue
        states = section.get("block_states") or {}
        palette = states.get("palette", section.get("Palette", []))
        names = palette_names(palette)
        if not names:
            continue
        if len(names) == 1:
            if names[0] not in ("minecraft:air", "minecraft:cave_air", "minecraft:void_air"):
                block_y = section_y * 16 + 15
                for i, old in enumerate(best):
                    if old is None or block_y > old[0]:
                        best[i] = (block_y, names[0])
            continue
        longs = states.get("data", section.get("BlockStates"))
        if longs is None:
            continue
        bits = max(4, (len(names) - 1).bit_length())
        per_long = 64 // bits
        mask = (1 << bits) - 1
        if len(longs) < (4096 + per_long - 1) // per_long:
            continue
        for y in range(16):
            for z in range(16):
                for x in range(16):
                    i = (y * 16 + z) * 16 + x
                    word = longs[i // per_long] & ((1 << 64) - 1)
                    palette_index = (word >> ((i % per_long) * bits)) & mask
                    if palette_index >= len(names):
                        continue
                    name = names[palette_index]
                    if name in ("minecraft:air", "minecraft:cave_air", "minecraft:void_air"):
                        continue
                    col = z * 16 + x
                    block_y = section_y * 16 + y
                    if best[col] is None or block_y > best[col][0]:
                        best[col] = (block_y, name)
    output = []
    ox, oz = cx * 16, cz * 16
    for i, sample in enumerate(best):
        if sample is None:
            continue
        y, name = sample
        lx, lz = i % 16, i // 16
        output.append([
            ox + lx,
            oz + lz,
            y,
            name,
            name == "minecraft:lava",
            quart_surface_biome(data, sections, lx, y, lz),
        ])
    return output


def region_files(region_dir, sites, chunk_radius):
    files = set()
    for _, x, z in sites:
        ccx, ccz = x // 16, z // 16
        for dz in range(-chunk_radius, chunk_radius + 1):
            for dx in range(-chunk_radius, chunk_radius + 1):
                cx, cz = ccx + dx, ccz + dz
                rx, rz = cx // 32, cz // 32
                files.add(region_dir / f"r.{rx}.{rz}.mca")
    return sorted(files)


def stamps(paths):
    result = {}
    for path in paths:
        if not path.exists():
            result[path.name] = None
            continue
        stat = path.stat()
        result[path.name] = {
            "bytes": stat.st_size,
            "mtime_ns": stat.st_mtime_ns,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--world", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--chunk-radius", type=int, default=1)
    args = parser.parse_args()
    if args.chunk_radius < 0 or args.chunk_radius > 2:
        parser.error("chunk radius must be in 0..=2")
    region_dir = args.world / "region"
    files = region_files(region_dir, SITES, args.chunk_radius)
    before = stamps(files)
    rows = []
    for name, x, z in SITES:
        center_cx, center_cz = x // 16, z // 16
        palette_blocks = set()
        palette_biomes = set()
        columns = []
        full_chunks = []
        missing_chunks = []
        for dz in range(-args.chunk_radius, args.chunk_radius + 1):
            for dx in range(-args.chunk_radius, args.chunk_radius + 1):
                cx, cz = center_cx + dx, center_cz + dz
                root = chunk_nbt(region_dir, cx, cz)
                if root is None:
                    missing_chunks.append([cx, cz])
                    continue
                extracted = block_columns(root, cx, cz)
                if extracted is None:
                    missing_chunks.append([cx, cz])
                    continue
                full_chunks.append([cx, cz])
                for col in extracted:
                    palette_blocks.add(col[3])
                    if col[5] is not None:
                        palette_biomes.add(col[5])
                    columns.append(col)
        rows.append({
            "family": name,
            "center": [x, z],
            "center_chunk": [center_cx, center_cz],
            "radius_chunks": args.chunk_radius,
            "complete_chunks": sorted(full_chunks),
            "missing_or_incomplete_chunks": sorted(missing_chunks),
            "block_palette": sorted(palette_blocks),
            "biome_palette": sorted(palette_biomes),
            "columns": columns,
        })
    after = stamps(files)
    if before != after:
        raise SystemExit("Saved region inputs changed during extraction; discard this result and retry after saving settles.")
    all_cols = [c for site in rows for c in site["columns"]]
    output = {
        "schema": 1,
        "provenance": {
            "kind": "read_only_generated_chunk_palette_and_surface_block_capture",
            "seed": 0,
            "dimension": "minecraft:overworld",
            "chunk_radius": args.chunk_radius,
            "sampling": "All columns of every fully generated chunk within the bounded square around each saved volcanic center; no model based point selection.",
            "notes": [
                "Each column row is [block_x, block_z, highest_non_air_y, block_id, exact_surface_lava, saved_quart_biome_at_surface].",
                "Lava means the actual highest non-air saved block is minecraft:lava; it does not infer exposed magma below snow or rock.",
                "No Minecraft launch, chunk generation or save writes.",
            ],
            "stable_during_extraction": True,
            "region_sha256_before_after": before,
            "extraction_script": "native/terralens-rs/tools/reference/extract_volcanic_surface_fluids.py",
            "captured_unix_ns": time.time_ns(),
        },
        "sites": rows,
    }
    args.output.write_text(json.dumps(output, separators=(",", ":")) + "\n")
    for site in rows:
        lava = [col for col in site["columns"] if col[4]]
        print(f"{site['family']}: complete={len(site['complete_chunks'])} missing={len(site['missing_or_incomplete_chunks'])} columns={len(site['columns'])} lava_tops={len(lava)}")
    print(f"total: chunks={sum(len(s['complete_chunks']) for s in rows)} columns={len(all_cols)} file={args.output}")


if __name__ == "__main__":
    main()
