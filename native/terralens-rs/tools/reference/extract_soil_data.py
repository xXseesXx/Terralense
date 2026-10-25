#!/usr/bin/env python3
"""Extract the effective TFG soil-disc placement/configuration profile.

Usage: python3 tools/reference/extract_soil_data.py /path/to/tfg-core-0.9.25 [output.json]
The placed feature's configured-feature reference is followed deliberately; this
preserves pack data quirks such as duff_disc_old_growth linking to duff_disc_normal.
"""
import json
import pathlib
import sys

if len(sys.argv) not in (2, 3):
    raise SystemExit(__doc__)
source = pathlib.Path(sys.argv[1]) / "src/main/resources/data/tfg/worldgen"
output = pathlib.Path(sys.argv[2]) if len(sys.argv) == 3 else pathlib.Path(__file__).parents[2] / "src/ground/decorations/assets/soils.json"
order_path = pathlib.Path(__file__).parents[2] / "src/vegetation/assets/feature-order.json"
feature_order = json.loads(order_path.read_text())["stages"]["8"]
ids = [
    "tfg:earth/duff_disc_edge",
    "tfg:earth/duff_disc_normal",
    "tfg:earth/duff_disc_old_growth",
    "tfg:earth/alfisol_disc",
    "tfg:earth/podzol_disc",
]
result = {"source": "TFG Core 0.9.25 effective placed/configured feature data", "features": []}
for feature_id in ids:
    name = feature_id.rsplit("/", 1)[1]
    placed = json.loads((source / f"placed_feature/earth/{name}.json").read_text())
    configured_id = placed["feature"]
    configured_name = configured_id.rsplit("/", 1)[1]
    config = json.loads((source / f"configured_feature/earth/{configured_name}.json").read_text())["config"]
    climate, count = placed["placement"][:2]
    result["features"].append({
        "id": feature_id,
        "index": feature_order.index(feature_id),
        "configured_feature": configured_id,
        "count": count["count"],
        "min_forest": climate.get("min_forest", "none"),
        "max_forest": climate.get("max_forest", "old_growth"),
        "min_temperature": climate.get("min_temperature", -3.4028234663852886e38),
        "max_temperature": climate.get("max_temperature", 3.4028234663852886e38),
        "min_rainfall": climate.get("min_rainfall", -3.4028234663852886e38),
        "max_rainfall": climate.get("max_rainfall", 3.4028234663852886e38),
        "fuzzy": climate.get("fuzzy", False),
        "min_radius": config["min_radius"],
        "max_radius_exclusive": config["max_radius"],
        "height": config["height"],
        "states": {state["replace"]: state["with"] for state in config["states"]},
    })
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(result, indent=2) + "\n")
