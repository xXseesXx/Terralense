#!/usr/bin/env python3
"""Reproduce GTCEu's TFG bedrock-fluid registry bucket order.

This models the Java 17 HashMap operations used by GTCEu 7.5.3:
GTBedrockFluids.toReRegister starts with its eight built-ins; TFG's mixin
triggers the 41 TFGBedrockFluids declarations and removes those built-ins;
the remaining values are registered into GTRegistry.keyToValue; then KubeJS
registers the nine Moon/Mars/Venus entries. ResourceLocation.hashCode is
31 * namespace.hashCode() + path.hashCode().

Example:
  python tools/reference/extract_fluid_veins.py \
    --tfg-source /tmp/tfg-core-0.9.25/src/main/java \
    --kubejs-root ~/.local/share/PrismLauncher/instances/TerraFirmaGreg-Modern/minecraft/kubejs/server_scripts
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


GT_BUILTINS = [
    "gtceu:heavy_oil_deposit",
    "gtceu:light_oil_deposit",
    "gtceu:natural_gas_deposit",
    "gtceu:oil_deposit",
    "gtceu:raw_oil_deposit",
    "gtceu:salt_water_deposit",
    "gtceu:lava_deposit",
    "gtceu:nether_natural_gas_deposit",
]


def java_string_hash(value: str) -> int:
    result = 0
    for char in value:
        result = (31 * result + ord(char)) & 0xFFFFFFFF
    return result if result < 0x80000000 else result - 0x100000000


def location_hash(value: str) -> int:
    namespace, path = value.split(":", 1)
    return (31 * java_string_hash(namespace) + java_string_hash(path)) & 0xFFFFFFFF


class JavaHashMap:
    """Small Java HashMap model; none of these inventories treeify a bucket."""

    def __init__(self) -> None:
        self.capacity = 16
        self.threshold = 12
        self.buckets: list[list[tuple[int, str]]] = [[] for _ in range(self.capacity)]
        self.size = 0

    @staticmethod
    def spread(key_hash: int) -> int:
        return (key_hash ^ (key_hash >> 16)) & 0xFFFFFFFF

    def put(self, key: str) -> None:
        h = self.spread(location_hash(key))
        bucket = self.buckets[h & (self.capacity - 1)]
        if any(old_key == key for _, old_key in bucket):
            return
        bucket.append((h, key))
        self.size += 1
        if self.size > self.threshold:
            self.resize()

    def remove(self, key: str) -> None:
        h = self.spread(location_hash(key))
        bucket = self.buckets[h & (self.capacity - 1)]
        for index, (_, old_key) in enumerate(bucket):
            if old_key == key:
                del bucket[index]
                self.size -= 1
                return

    def resize(self) -> None:
        old = self.buckets
        self.capacity *= 2
        self.threshold = self.capacity * 3 // 4
        new: list[list[tuple[int, str]]] = [[] for _ in range(self.capacity)]
        for index, bucket in enumerate(old):
            low: list[tuple[int, str]] = []
            high: list[tuple[int, str]] = []
            for entry in bucket:
                (high if entry[0] & (self.capacity // 2) else low).append(entry)
            new[index] = low
            new[index + self.capacity // 2] = high
        self.buckets = new

    def values(self) -> list[str]:
        return [key for bucket in self.buckets for _, key in bucket]


def extract(tfg_source: Path, kubejs_root: Path) -> dict[str, object]:
    tfg_file = tfg_source / "su/terrafirmagreg/core/common/data/tfgt/TFGBedrockFluids.java"
    declarations = re.findall(r'create\(TFGCore\.id\("([^"]+)"\)', tfg_file.read_text())
    if len(declarations) != 41:
        raise SystemExit(f"Expected 41 TFG fluid declarations, found {len(declarations)}")

    tfg_keys = [f"tfg:{path}" for path in declarations]
    to_reregister = JavaHashMap()
    for key in GT_BUILTINS + tfg_keys:
        to_reregister.put(key)
    for key in GT_BUILTINS:
        to_reregister.remove(key)
    register_order = to_reregister.values()

    kubejs_order: list[str] = []
    for planet, filename in [
        ("moon", "data.moon.js"),
        ("mars", "data.mars.js"),
        ("venus", "data.venus.js"),
    ]:
        script = (kubejs_root / "tfg" / planet / filename).read_text()
        kubejs_order.extend(re.findall(r"event\.add\(['\"]([^'\"]+)['\"]", script))
    if len(kubejs_order) != 9:
        raise SystemExit(f"Expected 9 planetary KubeJS fluid definitions, found {len(kubejs_order)}")

    registry = JavaHashMap()
    for key in register_order + kubejs_order:
        registry.put(key)
    result = registry.values()
    return {
        "source": {
            "tfg_core": "0.9.25",
            "gtceu": "7.5.3",
            "resource_location_hash": "31 * JavaStringHash(namespace) + JavaStringHash(path)",
            "kubejs_registration_order": ["moon", "mars", "venus"],
        },
        "to_re_register_capacity": to_reregister.capacity,
        "registry_capacity": registry.capacity,
        "registry_order": result,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tfg-source", required=True, type=Path)
    parser.add_argument("--kubejs-root", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    document = extract(args.tfg_source, args.kubejs_root)
    rendered = json.dumps(document, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered)
    else:
        print(rendered, end="")


if __name__ == "__main__":
    main()
