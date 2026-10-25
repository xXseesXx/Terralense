#!/usr/bin/env python3
"""Extract pinned TFG surface factory identities without interpreting builder code."""
import argparse, json, pathlib, re
p = argparse.ArgumentParser(); p.add_argument('source', type=pathlib.Path); p.add_argument('--output', type=pathlib.Path, default=pathlib.Path('src/ground/assets/surface-builders.json')); args = p.parse_args()
src = args.source / 'src/main/java/su/terrafirmagreg/core/world/new_ow_wg/biome/TFGBiomes.java'
text = src.read_text(); entries = {}
for m in re.finditer(r'public static final BiomeExtension\s+\w+\s*=\s*register\("([^\"]+)"', text):
    start = text.index('(', m.start()); depth = 1; i = start + 1
    while depth:
        if text[i] == '(': depth += 1
        elif text[i] == ')': depth -= 1
        i += 1
    expr = text[start:i]; surface = re.search(r'\.surface\(([^\n]+)\)', expr)
    if surface:
        factory = surface.group(1).strip()
        entries[m.group(1)] = {'factory': factory, 'normal_min_depth': -1 if factory == 'TFGNormalSurfaceBuilder.INSTANCE' else -3 if factory == 'TFGNormalSurfaceBuilder.ROCKY' else None, 'sandy_river_shores': '.noSandyRiverShores()' not in expr, 'is_shore': '.shore()' in expr}
assert len(entries) == 109, len(entries)
result = {'source': 'TFG Core 0.9.25 f4929e738e83dbc8d46680276faa67520dffa172 TFGBiomes.java', 'biomes': entries}
args.output.parent.mkdir(parents=True, exist_ok=True); args.output.write_text(json.dumps(result, indent=2, sort_keys=True)+'\n')
print(f'{len(entries)} biome surface factories')
