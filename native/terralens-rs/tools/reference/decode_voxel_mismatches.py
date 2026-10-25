#!/usr/bin/env python3
"""Read-only decode of actual Minecraft block IDs for the 488 voxel mismatch rows."""
import argparse, collections, gzip, hashlib, json, pathlib, sys
ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools/reference'))
import extract_volcanic_surface_fluids as anvil
ap=argparse.ArgumentParser(description=__doc__)
ap.add_argument('world',type=pathlib.Path)
ap.add_argument('--audit',type=pathlib.Path,default=ROOT.parents[1]/'docs/data/surface-mismatch-world0-before.json.gz')
ap.add_argument('--output',type=pathlib.Path,default=pathlib.Path('/tmp/terralens-surface-voxel-block-identities.json'))
args=ap.parse_args()
AUDIT,WORLD,OUT=args.audit,args.world,args.output

def classify(name):
    path = name.split(':', 1)[-1]
    if name in {'minecraft:air','minecraft:cave_air','minecraft:void_air'}: return 'air'
    if name in {'minecraft:snow','minecraft:snow_block','minecraft:ice','minecraft:packed_ice','minecraft:blue_ice','tfc:sea_ice'}: return 'excluded_snow_ice'
    if ('lava' in path and 'magma' not in path) or name in {'minecraft:lava','tfc:fluid/lava'}: return 'lava'
    if 'water' in path: return 'water'
    return 'solid'

def block_at(data, x, y, z):
    lx, lz = x % 16, z % 16
    for sec in data.get('sections', data.get('Sections', [])):
        if int(sec.get('Y', 99999)) != y // 16: continue
        states = sec.get('block_states') or {}
        palette = states.get('palette', sec.get('Palette', []))
        names = anvil.palette_names(palette)
        if not names: return None
        if len(names) == 1: return names[0]
        longs = states.get('data', sec.get('BlockStates'))
        if longs is None: return None
        bits = max(4, (len(names) - 1).bit_length())
        per_long = 64 // bits
        idx = ((y % 16) * 16 + lz) * 16 + lx
        word = longs[idx // per_long] & ((1 << 64) - 1)
        pi = (word >> ((idx % per_long) * bits)) & ((1 << bits) - 1)
        return names[pi] if pi < len(names) else None
    return None

report = json.loads(gzip.decompress(AUDIT.read_bytes())) if AUDIT.suffix=='.gz' else json.loads(AUDIT.read_text())
expected_hashes = report['reference']['region_sha256']
region_paths = {name: WORLD / 'region' / name for name in expected_hashes}
actual_hashes = {name: hashlib.sha256(path.read_bytes()).hexdigest() for name,path in region_paths.items()}
changed = [name for name in expected_hashes if actual_hashes[name] != expected_hashes[name]]
if changed:
    raise SystemExit(f'World 0 region inputs differ from audit provenance: {changed}')
chunks = {}
rows = []
class_mismatches = []
stage_block_counts = collections.Counter()
for row in report['voxel_rows']:
    cx, cz = row['chunk']
    key = (cx,cz)
    if key not in chunks:
        root = anvil.chunk_nbt(WORLD / 'region', cx, cz)
        data = root.get('Level', root)
        chunks[key] = data
    x,y,z = row['point']
    name = block_at(chunks[key], x,y,z)
    if name is None:
        raise SystemExit(f'missing block at {x},{y},{z}')
    actual_class = classify(name)
    fixture_class = row['actual']
    stage_block_counts[(row['modeled_divergence_stage'],name)] += 1
    if actual_class != fixture_class:
        class_mismatches.append({'point':[x,y,z], 'block':name, 'decoded_class':actual_class, 'fixture_class':fixture_class})
    rows.append({
        'source_chunk':[cx,cz], 'point':[x,y,z],
        'modeled_stage':row['modeled_divergence_stage'],
        'modeled_class':row['predicted'], 'fixture_class':fixture_class,
        'actual_block':name, 'decoded_class':actual_class,
        'fixture_class_matches_decoded':actual_class == fixture_class,
        'trace':row['trace'],
    })
summary = collections.Counter((r['modeled_stage'], r['actual_block']) for r in rows)
out = {
    'read_only': True,
    'world': str(WORLD),
    'audit_input': str(AUDIT),
    'rows': len(rows),
    'decoded_class_matches_fixture': len(rows)-len(class_mismatches),
    'decoded_class_mismatches': class_mismatches,
    'region_hashes_match_audit_provenance': not changed,
    'region_sha256': actual_hashes,
    'counts_by_modeled_stage_and_actual_block': [
        {'modeled_stage':stage,'actual_block':block,'count':count}
        for (stage,block),count in sorted(summary.items())
    ],
    'voxel_rows': rows,
}
OUT.write_text(json.dumps(out, indent=2) + '\n')
print(json.dumps({
    'output':str(OUT), 'rows':len(rows),
    'decoded_class_matches_fixture':out['decoded_class_matches_fixture'],
    'decoded_class_mismatches':len(class_mismatches),
    'region_hashes_match_audit_provenance':not changed,
    'counts_by_stage_and_block':out['counts_by_modeled_stage_and_actual_block'],
}, indent=2))
