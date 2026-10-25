#!/usr/bin/env python3
"""Bundle compact candidate template geometry and container NBT for TFG structure previews.

This is a maintenance tool. It reads the installed pack archives and the reviewed
structure applicability inventory; the application never scans the Minecraft install.
"""
import argparse, gzip, hashlib, io, json, pathlib, re, zipfile
import nbtlib

ap = argparse.ArgumentParser(description=__doc__)
ap.add_argument('--minecraft-dir', type=pathlib.Path, required=True)
ap.add_argument('--minecraft-client-jar', type=pathlib.Path, required=True)
ap.add_argument('--structure-applicability', type=pathlib.Path, required=True)
ap.add_argument('--structure-loot', type=pathlib.Path, required=True)
ap.add_argument('--kubejs-data', type=pathlib.Path)
ap.add_argument('--output', type=pathlib.Path, required=True)
a = ap.parse_args()
applicability = json.loads(a.structure_applicability.read_text())
loot = json.loads(a.structure_loot.read_text())['structures']
ids = {entry['id'] for entry in applicability['structures']
       if any(x.get('scanner_eligible') for x in entry.get('dimensions', {}).values())}
templates = sorted({t for ident in ids for t in loot.get(ident, {}).get('templates', [])})
archives = [zipfile.ZipFile(a.minecraft_client_jar)]
for jar in sorted((a.minecraft_dir / 'mods').glob('*.jar')):
    try: archives.append(zipfile.ZipFile(jar))
    except (OSError, zipfile.BadZipFile): pass
raw_templates = {}
pool_resources = {}
processor_resources = {}
for z in archives:
    for item in z.namelist():
        if item.startswith('data/') and '/worldgen/template_pool/' in item and item.endswith('.json'):
            try:
                rel = item[5:]
                ns, rest = rel.split('/', 1)
                ident = ns + ':' + rest[len('worldgen/template_pool/'):-5]
                pool_resources.setdefault(ident, json.loads(z.read(item)))
            except Exception: pass
        if item.startswith('data/') and '/worldgen/processor_list/' in item and item.endswith('.json'):
            try:
                rel = item[5:]
                ns, rest = rel.split('/', 1)
                ident = ns + ':' + rest[len('worldgen/processor_list/'):-5]
                processor_resources.setdefault(ident, json.loads(z.read(item)))
            except Exception: pass
        if not item.startswith('data/') or '/structures/' not in item or not item.endswith('.nbt'):
            continue
        rel = item[5:]
        ns, rest = rel.split('/', 1)
        ident = ns + ':' + rest[len('structures/'):-4]
        if ident in templates:
            raw_templates[ident] = z.read(item)
if a.kubejs_data and a.kubejs_data.exists():
    for p in sorted(a.kubejs_data.glob('*/structures/**/*.nbt')):
        ns = p.relative_to(a.kubejs_data).parts[0]
        ident = ns + ':' + p.relative_to(a.kubejs_data / ns / 'structures').with_suffix('').as_posix()
        if ident in templates: raw_templates[ident] = p.read_bytes()
if a.kubejs_data and a.kubejs_data.exists():
    for p in sorted(a.kubejs_data.glob('*/worldgen/template_pool/**/*.json')):
        ns = p.relative_to(a.kubejs_data).parts[0]
        ident = ns + ':' + p.relative_to(a.kubejs_data / ns / 'worldgen/template_pool').with_suffix('').as_posix()
        pool_resources[ident] = json.loads(p.read_text())
    for p in sorted(a.kubejs_data.glob('*/worldgen/processor_list/**/*.json')):
        ns = p.relative_to(a.kubejs_data).parts[0]
        ident = ns + ':' + p.relative_to(a.kubejs_data / ns / 'worldgen/processor_list').with_suffix('').as_posix()
        processor_resources[ident] = json.loads(p.read_text())

def plain(v):
    if isinstance(v, (str, int, float, bool)) or v is None: return v
    if hasattr(v, 'items'): return {str(k): plain(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)): return [plain(x) for x in v]
    if hasattr(v, 'tolist'): return plain(v.tolist())
    try: return int(v)
    except (TypeError, ValueError): return str(v)

def read_nbt(raw):
    for opener in (gzip.GzipFile, lambda fileobj: fileobj):
        try:
            return nbtlib.File.parse(opener(fileobj=io.BytesIO(raw)))
        except Exception: pass
    raise ValueError('unreadable template')

def id_for(state):
    return str(state.get('Name', 'minecraft:air'))

out_templates = {}
for ident in templates:
    raw = raw_templates.get(ident)
    if raw is None: continue
    try: root = read_nbt(raw)
    except Exception: continue
    palette = root.get('palette', [])
    block_names = [id_for(s) for s in palette]
    indices = {}
    for i, name in enumerate(block_names): indices.setdefault(name, i)
    blocks = []
    for b in root.get('blocks', []):
        pos = b.get('pos', (0, 0, 0)); state = int(b.get('state', 0))
        name = block_names[state] if 0 <= state < len(block_names) else 'minecraft:air'
        if name in ('minecraft:air', 'minecraft:cave_air', 'minecraft:void_air', 'minecraft:structure_void'):
            continue
        x, y, z = (int(pos[i]) for i in range(3))
        blocks.append([x, y, z, indices[name]])
    size = root.get('size', (0, 0, 0))
    containers = []
    for b in root.get('blocks', []):
        pos = b.get('pos', (0, 0, 0)); state = int(b.get('state', 0))
        name = block_names[state] if 0 <= state < len(block_names) else 'minecraft:air'
        nbt = b.get('nbt')
        path = name.split(':', 1)[-1].lower()
        tokens = set(re.split(r'[/_]+', path))
        is_container_block = bool(tokens & {'chest','barrel','vessel','vase','urn','jar','crate','shulker','placed'}) or any(k in path for k in ('decorated_pot','suspicious_sand','suspicious_gravel'))
        if not is_container_block:
            continue
        data = plain(nbt) if nbt is not None else {}
        items = data.get('Items', []) if isinstance(data, dict) else []
        inventory = data.get('inventory', {}) if isinstance(data, dict) else {}
        if isinstance(inventory, dict): items += inventory.get('Items', [])
        converted = []
        for item in items:
            if isinstance(item, dict) and item.get('id'):
                converted.append({'id': str(item['id']), 'count': int(item.get('Count', item.get('count', 1))), 'slot': item.get('Slot', item.get('slot'))})
        containers.append({'pos': [int(pos[i]) for i in range(3)], 'block': name,
            'loot_table': data.get('LootTable') if isinstance(data, dict) else None,
            'loot_seed': data.get('LootTableSeed') if isinstance(data, dict) else None,
            'items': converted})
    jigsaws = []
    for b in root.get('blocks', []):
        state = int(b.get('state', 0))
        if not (0 <= state < len(palette)) or block_names[state] != 'minecraft:jigsaw': continue
        pos = b.get('pos', (0, 0, 0)); data = plain(b.get('nbt')) or {}
        props = plain(palette[state].get('Properties', {}))
        if not isinstance(data, dict): data = {}
        jigsaws.append({'pos': [int(pos[i]) for i in range(3)], 'name': str(data.get('name', 'minecraft:empty')),
            'target': str(data.get('target', 'minecraft:empty')), 'pool': str(data.get('pool', 'minecraft:empty')),
            'joint': str(data.get('joint', 'rollable')), 'orientation': str(props.get('orientation', 'north_up')),
            'final_state': str(data.get('final_state', 'minecraft:air'))})
    # Template pools are alternative pieces. Keep alternatives, never merge their blocks.
    out_templates[ident] = {'size': [int(v) for v in size], 'palette': block_names,
                            'blocks': blocks, 'containers': containers, 'jigsaws': jigsaws,
                            'sha256': hashlib.sha256(raw).hexdigest()}
out_structures = {}
for ident in sorted(ids):
    entry = loot.get(ident, {})
    available = [t for t in entry.get('templates', []) if t in out_templates]
    definition = next((v['definition'] for v in applicability['structures'] if v['id'] == ident), {})
    pool = definition.get('start_pool')
    out_structures[ident] = {'start_pool': pool, 'size': int(definition.get('size', 0)),
                             'start_height': definition.get('start_height', {}),
                             'projection': definition.get('project_start_to_heightmap'),
                             'templates': available}
inline_processor_types = {}
def processor_id(value):
    if value is None: return None
    if isinstance(value, str): return value
    if isinstance(value, dict):
        types = sorted({str(p.get('processor_type')) for p in value.get('processors', []) if isinstance(p, dict) and p.get('processor_type')})
        key = 'terralens:inline:' + hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()[:16]
        inline_processor_types[key] = types
        return key
    return None
def element(e):
    if not isinstance(e, dict): return {'kind': 'unknown'}
    kind = e.get('element_type', 'minecraft:single_pool_element')
    if kind in ('minecraft:single_pool_element', 'minecraft:legacy_single_pool_element'):
        loc = e.get('location')
        if isinstance(loc, dict): loc = loc.get('id')
        return {'kind': 'single', 'template': loc, 'processors': processor_id(e.get('processors')), 'projection': e.get('projection'), 'ground_level_delta': int(e.get('ground_level_delta', 0))}
    if kind == 'minecraft:list_pool_element': return {'kind': 'list', 'elements': [element(x) for x in e.get('elements', [])], 'projection': e.get('projection')}
    if kind == 'lithostitched:limited':
        # Keep the delegated element and cap as data. The Rust assembler applies a
        # per-preview cap and marks the result incomplete because Lithostitched's
        # cross-pool/global accounting can depend on generation context.
        return {'kind': 'limited', 'delegate': element(e.get('delegate')), 'limit': int(e.get('limit', 1))}
    if kind == 'lithostitched:delegating':
        # This wrapper forwards template geometry/jigsaws to its delegate. The terrain
        # adaptation override changes integration with terrain, not its start-box center.
        return {**element(e.get('delegate')), 'terrain_adaptation': e.get('override_terrain_adaption')}
    if kind == 'minecraft:empty_pool_element': return {'kind': 'empty'}
    if kind == 'minecraft:feature_pool_element': return {'kind': 'feature'}
    return {'kind': 'unknown', 'type': kind}
out_pools = {}
out_processors = dict(inline_processor_types)
for ident, processor_list in sorted(processor_resources.items()):
    out_processors[ident] = sorted({str(p.get('processor_type')) for p in processor_list.get('processors', []) if isinstance(p, dict) and p.get('processor_type')})
for ident, pool in sorted(pool_resources.items()):
    entries = []
    for item in pool.get('elements', []):
        if not isinstance(item, dict): continue
        entries.append({'weight': int(item.get('weight', 1)), 'element': element(item.get('element', item))})
    fallback = pool.get('fallback', 'minecraft:empty')
    if isinstance(fallback, dict): fallback = fallback.get('id', 'minecraft:empty')
    out_pools[ident] = {'fallback': fallback, 'entries': entries}
out_processors.update(inline_processor_types)
document = {'format': 1, 'structures': out_structures, 'templates': out_templates, 'pools': out_pools, 'processors': out_processors}
raw = json.dumps(document, separators=(',', ':'), sort_keys=True).encode()
a.output.parent.mkdir(parents=True, exist_ok=True)
a.output.write_bytes(gzip.compress(raw, compresslevel=9, mtime=0))
print(f'{len(out_structures)} structures; {len(out_templates)} templates; {len(raw):,} JSON bytes; {a.output.stat().st_size:,} gzip bytes')
