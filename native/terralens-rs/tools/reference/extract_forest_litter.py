#!/usr/bin/env python3
"""Extract TFC forest groundcover block providers from a pinned TFG source tree.

Usage: python3 tools/reference/extract_forest_litter.py /tmp/tfg-core-0.9.25
"""
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
source = root / "src/main/resources/data/tfc/worldgen/configured_feature/tree"
out = Path(__file__).parents[2] / "src/vegetation/assets/forest_litter.json"
entries = {}
for path in sorted(source.glob("*_entry.json")):
    payload = json.loads(path.read_text())
    config = payload.get("config", {})
    groundcover = []
    for item in config.get("groundcover", []):
        block = item.get("block")
        if block:
            groundcover.append({"block": block, "weight": item.get("weight", 1.0)})
    entries[f"tfc:tree/{path.stem}"] = {
        "groundcover": groundcover,
        "fallen_leaves": config.get("fallen_leaves"),
        "bush_log": config.get("bush_log", {}).get("Name"),
        "bush_leaves": config.get("bush_leaves"),
        "climate": config.get("climate", {}),
    }
out.write_text(json.dumps({"source": "TFG Core 0.9.25 TFC configured_feature/tree/*_entry.json", "entries": entries}, indent=2, sort_keys=True) + "\n")
print(f"wrote {len(entries)} forest litter entries to {out}")
