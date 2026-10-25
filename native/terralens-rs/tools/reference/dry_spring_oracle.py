#!/usr/bin/env python3
"""Run the independent Java basin control over captured World 0 noise heights.

No Rust code, Minecraft process or live save is involved. The input is NOT a
capture of the mutable pre-spring world; earlier feature interactions are absent.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture = ROOT / "tests/fixtures/ground/dike-user-site-seed0.json"
    source = ROOT / "src/hot_springs/reference/HotSpringJavaReference.java"
    data = json.loads(fixture.read_text())
    columns = [r for c in data["chunks"] for r in c["columns"]]
    with tempfile.TemporaryDirectory(prefix="terralens-spring-oracle-") as directory:
        heights = Path(directory) / "heights.csv"
        heights.write_text("".join(
            f"{r['world_x']},{r['world_z']},{r['surface_height_forgecaps']}\n"
            for r in columns
        ))
        subprocess.run(["javac", "-d", directory, str(source)], check=True)
        result = subprocess.run(
            ["java", "-cp", directory, "HotSpringJavaReference", str(heights)],
            check=True, capture_output=True, text=True,
        )
    rows = []
    lookup = {}
    for line in result.stdout.splitlines():
        x, z, y, kind = line.split(",")
        row = [int(x), int(z), int(y), kind]
        rows.append(row)
        lookup[row[0], row[1]] = row
    rocks = [r for r in columns if r["ground_block"].startswith("tfc:rock/")]
    mismatches = []
    for saved in rocks:
        row = lookup.get((saved["world_x"], saved["world_z"]))
        if row is None or f"/{row[3]}/" not in saved["ground_block"]:
            mismatches.append({"saved": saved, "java_basin": row})
    report = {
        "scope": "Standalone Java source transcription, captured noise-height input; no preceding features, final fissures, fluid ticks or live Forge execution.",
        "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(),
        "java_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "profile": "stage 8, index 30; source chunk -19954,1006; origin -319250,16107",
        "continuation": result.stderr.strip(),
        "basin_columns": len(rows), "saved_exposed_rock_columns": len(rocks),
        "matching_exposed_rock_materials": len(rocks) - len(mismatches),
        "mismatches": mismatches, "rows": rows,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Java basin: {len(rows)} columns; {len(rocks)-len(mismatches)}/{len(rocks)} exposed rock categories match")


if __name__ == "__main__":
    main()
