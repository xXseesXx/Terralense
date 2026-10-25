#!/usr/bin/env python3
"""Read installed Minecraft structure pools/templates and inventory loot-capable blocks."""
import argparse
import gzip
import hashlib
import io
import json
import pathlib
import re
import zipfile
from collections import Counter, defaultdict, deque

import nbtlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--minecraft-dir', type=pathlib.Path, required=True)
parser.add_argument('--minecraft-client-jar', type=pathlib.Path, required=True)
parser.add_argument('--structure-applicability', type=pathlib.Path, required=True)
parser.add_argument('--output', type=pathlib.Path, required=True)
args = parser.parse_args()

base = args.minecraft_dir
kube = base / 'kubejs'
mods = base / 'mods'
archives = [('minecraft-1.20.1-client.jar', zipfile.ZipFile(args.minecraft_client_jar))]
for path in sorted(mods.glob('*.jar')):
    try:
        archives.append((path.name, zipfile.ZipFile(path)))
    except Exception:
        pass

resources = {kind: {} for kind in ('template_pool', 'template', 'processor_list', 'processor_tag')}
source_hashes = {}

def hash_source(name, raw):
    source_hashes[name] = hashlib.sha256(raw).hexdigest()

color_constants = {}
color_constants_file = kube / 'startup_scripts/minecraft/constants.js'
if color_constants_file.exists():
    constants_text = color_constants_file.read_text(errors='replace')
    for match in re.finditer(r'global\.(\w+)\s*=\s*\[(.*?)\]\s*;', constants_text, re.S):
        color_constants[match.group(1)] = re.findall(r"['\"]([^'\"]+)['\"]", match.group(2))
    rel='KubeJS/' + color_constants_file.relative_to(kube).as_posix()
    hash_source(rel,color_constants_file.read_bytes())

block_loot_modifiers = defaultdict(list)
for path in sorted((kube / 'server_scripts').rglob('*.js')):
    text = path.read_text(errors='replace')
    rel='KubeJS/' + path.relative_to(kube).as_posix()
    if 'addBlockLootModifier' not in text:
        continue
    hash_source(rel,path.read_bytes())
    for match in re.finditer(r"event\.addBlockLootModifier\(\s*(['\"])([^'\"]+)\1\s*\)",text):
        block_loot_modifiers[match.group(2)].append(f'{rel}:{text.count(chr(10),0,match.start())+1}')
    for match in re.finditer(r"event\.addBlockLootModifier\(\s*`([^`]+)`\s*\)",text):
        expression=match.group(1)
        variables=re.findall(r'\$\{(\w+)\}',expression)
        if not variables:
            block_loot_modifiers[expression].append(f'{rel}:{text.count(chr(10),0,match.start())+1}')
            continue
        values=color_constants.get(variables[0])
        if values is None and variables[0]=='color':
            values=color_constants.get('MINECRAFT_DYE_NAMES')
        if values is not None and len(variables)==1:
            for value in values:
                block_name=re.sub(r'\$\{'+re.escape(variables[0])+r'\}',value,expression)
                block_loot_modifiers[block_name].append(f'{rel}:{text.count(chr(10),0,match.start())+1}')

def put(kind, ident, value, source, raw=None, local=False, loader=None):
    prev = resources[kind].get(ident)
    # KubeJS data overrides all mod archives; otherwise retain the latest deterministic jar entry.
    if prev is None or local or not prev.get('local'):
        resources[kind][ident] = {'value': value, 'source': source, 'raw': raw, 'local': local, 'loader': loader}

def read_nbt(data):
    stream = io.BytesIO(data)
    try:
        return nbtlib.File.parse(gzip.GzipFile(fileobj=stream))
    except Exception:
        stream.seek(0)
        return nbtlib.File.parse(stream)

def id_for(path, kind):
    patterns = {
        'template_pool': r'data/([^/]+)/worldgen/template_pool/(.+)\.json$',
        'template': r'data/([^/]+)/structures/(.+)\.nbt$',
        'processor_list': r'data/([^/]+)/worldgen/processor_list/(.+)\.json$',
        'processor_tag': r'data/([^/]+)/tags/worldgen/processor_list/(.+)\.json$',
    }
    m = re.match(patterns[kind], path)
    return m.group(1) + ':' + m.group(2) if m else None

for name, archive in archives:
    relevant = []
    for item in archive.namelist():
        for kind in resources:
            ident = id_for(item, kind)
            if not ident:
                continue
            relevant.append(item)
            try:
                if kind == 'template':
                    put(kind, ident, None, name + '!/' + item, loader=lambda z=archive,n=item: read_nbt(z.read(n)))
                else:
                    raw = archive.read(item)
                    put(kind, ident, json.loads(raw), name + '!/' + item, raw=raw)
            except Exception as exc:
                put(kind, ident, None, name + '!/' + item, raw=None)
    if relevant:
        source_hashes[name] = hashlib.sha256((mods / name if name != 'minecraft-1.20.1-client.jar' else args.minecraft_client_jar).read_bytes()).hexdigest()

# Current local KubeJS data takes priority over every jar resource.
for p in (kube / 'data').glob('*/worldgen/template_pool/**/*.json'):
    ns = p.relative_to(kube / 'data').parts[0]
    ident = ns + ':' + p.relative_to(kube / 'data' / ns / 'worldgen' / 'template_pool').with_suffix('').as_posix()
    raw = p.read_bytes(); rel='KubeJS/' + p.relative_to(kube).as_posix(); hash_source(rel,raw); put('template_pool', ident, json.loads(raw), rel, raw, True)
for p in (kube / 'data').glob('*/structures/**/*.nbt'):
    ns = p.relative_to(kube / 'data').parts[0]
    ident = ns + ':' + p.relative_to(kube / 'data' / ns / 'structures').with_suffix('').as_posix()
    rel='KubeJS/' + p.relative_to(kube).as_posix(); hash_source(rel,p.read_bytes()); put('template', ident, None, rel, local=True, loader=lambda p=p: read_nbt(p.read_bytes()))
for p in (kube / 'data').glob('*/worldgen/processor_list/**/*.json'):
    ns = p.relative_to(kube / 'data').parts[0]
    ident = ns + ':' + p.relative_to(kube / 'data' / ns / 'worldgen' / 'processor_list').with_suffix('').as_posix()
    raw = p.read_bytes(); rel='KubeJS/' + p.relative_to(kube).as_posix(); hash_source(rel,raw); put('processor_list', ident, json.loads(raw), rel, raw, True)
for p in (kube / 'data').glob('*/tags/worldgen/processor_list/**/*.json'):
    ns = p.relative_to(kube / 'data').parts[0]
    ident = ns + ':' + p.relative_to(kube / 'data' / ns / 'tags' / 'worldgen' / 'processor_list').with_suffix('').as_posix()
    raw = p.read_bytes(); rel='KubeJS/' + p.relative_to(kube).as_posix(); hash_source(rel,raw); put('processor_tag', ident, json.loads(raw), rel, raw, True)

def as_id(v):
    if isinstance(v, str): return v
    if isinstance(v, dict): return v.get('id')
    return None

def is_container_name(name):
    lname=name.lower(); path_name=name.split(':',1)[-1]
    words=set(re.split(r'[/_]+',path_name))
    return bool(words & {'chest','barrel','vessel','vase','urn','jar','crate','shulker'}) or any(t in lname for t in ('suspicious_sand','suspicious_gravel','decorated_pot'))

def refs_in_pool(obj):
    """Return template refs, processor refs, nested pool refs and unsupported elements."""
    templates, processors, nested, unknown = [], [], [], []
    def walk(e):
        if not isinstance(e, dict):
            unknown.append('non-object pool element'); return
        et = e.get('element_type')
        if et in ('minecraft:single_pool_element', 'minecraft:legacy_single_pool_element'):
            loc = as_id(e.get('location'))
            if loc and loc!='minecraft:empty': templates.append(loc)
            elif loc=='minecraft:empty': pass
            else: unknown.append('single_pool_element without location')
            pr = as_id(e.get('processors'))
            if pr and pr not in ('minecraft:empty', 'minecraft:empty_processor_list'):
                processors.append(pr)
        elif et in ('minecraft:list_pool_element','lithostitched:limited','lithostitched:delegating'):
            if et in ('lithostitched:limited','lithostitched:delegating'):
                if 'delegate' in e: walk(e['delegate'])
                else: unknown.append(et + ' element without delegate')
                return
            for child in e.get('elements', []): walk(child)
        elif et in ('minecraft:empty_pool_element', None):
            if et is None: unknown.append('pool element missing element_type')
        elif et == 'minecraft:feature_pool_element':
            unknown.append('feature_pool_element: ' + str(e.get('feature')))
        else:
            unknown.append('unsupported element_type: ' + str(et))
    for entry in obj.get('elements', []):
        element = entry.get('element', entry) if isinstance(entry, dict) else entry
        walk(element)
    fallback = as_id(obj.get('fallback'))
    return templates, processors, ([fallback] if fallback and fallback != 'minecraft:empty' else []), unknown

def processor_data(ref, seen=None):
    """Resolve processor list references/tags, collecting loot modifiers and uncertainty."""
    seen = set() if seen is None else seen
    loot = []; types = []; unknown = []; containers = []
    if not ref: return loot, types, unknown
    if ref.startswith('#'):
        tag = ref[1:]
        if tag in seen: return loot, types, ['processor tag cycle: ' + tag], containers
        entry = resources['processor_tag'].get(tag)
        if not entry or not entry['value']: return loot, types, ['missing processor tag: ' + tag], containers
        values = entry['value'].get('values', [])
        for child in values:
            a,b,c,d = processor_data(as_id(child), seen | {tag}); loot += a; types += b; unknown += c; containers += d
        return loot, types, unknown, containers
    entry = resources['processor_list'].get(ref)
    if not entry or not entry['value']:
        return loot, types, ['missing processor list: ' + ref], containers
    value = entry['value']
    def walk(v):
        if isinstance(v, dict):
            ptype = v.get('processor_type')
            if ptype:
                types.append(ptype)
                if ptype not in {'minecraft:rule','minecraft:block_rot','minecraft:protected_blocks','minecraft:capped','minecraft:gravity','minecraft:ignore_waterlogging','minecraft:lava_submerged_block','minecraft:append_loot','lithostitched:block_swap','lithostitched:apply_random','ad_astra:structure_void_processor'}:
                    unknown.append('unreviewed processor type: ' + ptype)
            output = v.get('output_state')
            if isinstance(output,dict) and isinstance(output.get('Name'),str) and is_container_name(output['Name']):
                containers.append(output['Name'])
            bem = v.get('block_entity_modifier')
            if isinstance(bem, dict):
                btype = bem.get('type')
                if btype == 'minecraft:append_loot':
                    output = v.get('output_state', {})
                    loot.append({'type': btype, 'loot_table': bem.get('loot_table'), 'block':output.get('Name') if isinstance(output,dict) else None, 'processor_list': ref, 'source': entry['source']})
                elif btype: unknown.append('unreviewed block entity modifier: ' + str(btype))
            for child in v.values(): walk(child)
        elif isinstance(v, list):
            for child in v: walk(child)
    walk(value)
    return loot, list(dict.fromkeys(types)), list(dict.fromkeys(unknown)), list(dict.fromkeys(containers))

def plain(v):
    if isinstance(v, dict): return {str(k):plain(x) for k,x in v.items()}
    if isinstance(v, (list,tuple)): return [plain(x) for x in v]
    if isinstance(v, (str,int,float,bool)) or v is None: return v
    return str(v)

def nested_values(value, wanted):
    found=[]
    if isinstance(value, dict):
        for key, child in value.items():
            if str(key).lower() in wanted:
                found.append(child)
            found.extend(nested_values(child, wanted))
    elif isinstance(value, list):
        for child in value: found.extend(nested_values(child, wanted))
    return found

def nested_item_ids(value):
    ids=[]
    if isinstance(value, dict):
        ident=value.get('id')
        if isinstance(ident,str): ids.append(ident)
        for child in value.values(): ids.extend(nested_item_ids(child))
    elif isinstance(value,list):
        for child in value: ids.extend(nested_item_ids(child))
    return ids

def inspect_template(ident):
    entry = resources['template'].get(ident)
    if not entry or entry['value'] is None:
        if not entry or not entry.get('loader'): return None
        try: entry['value'] = entry['loader']()
        except Exception: return None
    nbt = entry['value']
    palette = nbt.get('palette', [])
    blocks = nbt.get('blocks', [])
    counts = Counter(); loot_blocks = []; containers = []; jigsaw_pools = set()
    for block in blocks:
        idx = int(block.get('state', -1))
        state = palette[idx] if 0 <= idx < len(palette) else {}
        name = str(state.get('Name', 'unknown'))
        counts[name] += 1
        block_nbt = plain(block.get('nbt', {}))
        lname = name.lower()
        if isinstance(block_nbt, dict) and block_nbt.get('id') == 'minecraft:jigsaw':
            pool = block_nbt.get('pool')
            if pool and pool != 'minecraft:empty': jigsaw_pools.add(str(pool))
        is_container = is_container_name(name)
        loot_tables = nested_values(block_nbt, {'loottable','loot_table'})
        loot_table = next((x for x in loot_tables if x), None)
        items_lists = nested_values(block_nbt, {'items'})
        items = next((x for x in items_lists if x), None)
        embedded_vessel = name == 'tfc:placed_item' and any('vessel' in x for x in nested_item_ids(block_nbt))
        effective_container = is_container or embedded_vessel
        item_contents = []
        if isinstance(items, list):
            for item in items:
                if isinstance(item, dict):
                    item_contents.append({'id':item.get('id'),'count':item.get('Count',item.get('count')),'tag':item.get('tag')})
        modifier_sources=block_loot_modifiers.get(name,[])
        if effective_container:
            containers.append({'block': name, 'embedded_vessel':embedded_vessel, 'loot_table': str(loot_table) if loot_table else None, 'has_nbt_items': bool(items), 'items':item_contents, 'block_loot_modifiers':modifier_sources, 'count':1})
        if loot_table or (items and effective_container) or modifier_sources:
            loot_blocks.append({'block':name,'loot_table':str(loot_table) if loot_table else None,'has_nbt_items':bool(items),'items':item_contents,'block_loot_modifiers':modifier_sources})
    # Compacted block-level counts keep the manifest useful without duplicating coordinates.
    c = Counter((x['block'],x['embedded_vessel'],x['loot_table'],x['has_nbt_items'],json.dumps(x['items'],sort_keys=True),json.dumps(x['block_loot_modifiers'])) for x in containers)
    containers = [{'block':k[0],'embedded_vessel':k[1],'loot_table':k[2],'has_nbt_items':k[3],'items':json.loads(k[4]),'block_loot_modifiers':json.loads(k[5]),'count':v} for k,v in sorted(c.items(),key=lambda kv:json.dumps(kv[0],sort_keys=True))]
    return {'id':ident,'source':entry['source'],'block_count':len(blocks),'block_types':len(counts),'loot_blocks':loot_blocks,'containers':containers,'jigsaw_pools':sorted(jigsaw_pools)}

app = json.loads(args.structure_applicability.read_text())
pool_cache = {}
templates = {}
records = []
processor_cache = {}
for structure in app['structures']:
    sid = structure['id']; definition = structure.get('definition') or {}
    start = definition.get('start_pool')
    queue = deque([start] if start else [])
    seen_pools = set(); pool_unknown = []; used_processors = set(); used_template_ids = set()
    while queue:
        pid = queue.popleft()
        if pid in seen_pools: continue
        seen_pools.add(pid)
        entry = resources['template_pool'].get(pid)
        if not entry or not entry['value']:
            pool_unknown.append('missing template pool: ' + pid); continue
        trefs, prefs, nested, unknown = refs_in_pool(entry['value'])
        pool_unknown.extend(f'{pid}: {x}' for x in unknown)
        queue.extend(nested)
        for pref in prefs: used_processors.add(pref)
        for tid in trefs:
            used_template_ids.add(tid)
            if tid not in templates:
                templates[tid] = inspect_template(tid)
            tpl = templates[tid]
            if tpl is None:
                pool_unknown.append('missing template: ' + tid)
            else:
                queue.extend(tpl['jigsaw_pools'])
    all_containers = Counter(); processor_containers=set(); loot_tables = set(); loot_evidence = []; unknown = list(pool_unknown)
    template_detail = []
    for tid in sorted(used_template_ids):
        tpl = templates.get(tid)
        if tpl is None: continue
        template_detail.append({'template':tid,'source':tpl['source'],'containers':tpl['containers'],'loot_blocks':tpl['loot_blocks'],'block_count':tpl['block_count']})
        for c in tpl['containers']:
            all_containers[c['block']] += c['count']
            if c['loot_table']: loot_tables.add(c['loot_table'])
        for lb in tpl['loot_blocks']:
            if lb['loot_table']: loot_tables.add(lb['loot_table'])
            evidence_kind='kubejs_block_loot_modifier' if lb.get('block_loot_modifiers') else 'template_block_nbt'
            loot_evidence.append({'kind':evidence_kind,'template':tid,**lb})
    processor_summaries = []
    for pref in sorted(used_processors):
        got = processor_data(pref)
        processor_cache[pref] = got
        plo, ptypes, punknown, pcontainers = got
        loot_evidence.extend({'kind':'processor_block_entity_modifier',**x} for x in plo)
        for x in plo:
            if x.get('loot_table'): loot_tables.add(x['loot_table'])
            if x.get('block'): processor_containers.add(x['block'])
        processor_containers.update(pcontainers)
        unknown.extend(punknown)
        processor_summaries.append({'id':pref,'processor_types':ptypes,'loot_modifiers':plo,'container_outputs':pcontainers,'source':resources['processor_list'].get(pref,{}).get('source')})
    unknown = list(dict.fromkeys(unknown))
    has_loot = bool(loot_tables or loot_evidence)
    has_containers = bool(all_containers or processor_containers)
    if has_loot:
        status = 'loot'
    elif not start:
        status = 'unknown'; unknown.append('structure has no data-defined start_pool; loot behavior may be hardcoded')
    elif has_containers:
        status = 'unknown'; unknown.append('reachable container has no fixed LootTable or Items NBT')
    elif unknown:
        status = 'unknown'
    else:
        status = 'no_loot'
    dim = structure.get('dimensions', {})
    records.append({
        'id':sid,'sets':structure.get('sets',[]),'type':definition.get('type'),'start_pool':start,
        'dimensions':{k:{'source_eligible':v.get('source_eligible',False),'scanner_eligible':v.get('scanner_eligible',False)} for k,v in dim.items()},
        'status':status,'has_loot':True if status=='loot' else (False if status=='no_loot' else None),
        'has_containers':has_containers,'reachable_pool_count':len(seen_pools),'reachable_pools':sorted(seen_pools),
        'container_blocks':sorted(set(all_containers)|processor_containers),'processor_container_blocks':sorted(processor_containers),'templates':sorted(used_template_ids),'unknowns':unknown,
        'reachable_template_count':len(used_template_ids),'reachable_templates':template_detail,
        'container_block_counts':dict(sorted(all_containers.items())),
        'loot_tables':sorted(loot_tables),'loot_evidence':loot_evidence,
        'processors':processor_summaries,'unknown_reasons':unknown,
    })

out = {
    'format':1,
    'provenance':{
        'source':'Installed Prism TFG Modern Minecraft 1.20.1 client/mod resources and KubeJS data; no game launch or save writes.',
        'structure_registry':'Current assets/data/tfg/structure-applicability.json (131 definitions, all supported dimensions).',
        'resource_precedence':'KubeJS data overrides mod jar resources by resource ID; mod jars are read in filename order where duplicate jar resources exist.',
        'pool_walk':'Start pool, fallback pools, list/single pool elements, and all non-empty jigsaw connector pools referenced by reachable templates are followed. This is an over-approximation of randomized jigsaw paths.',
        'classification':'loot means a reachable template has LootTable/Items NBT, a processor appends loot, or a pack KubeJS block loot modifier supplies loot to a reachable container. no_loot is emitted only when all reachable pools/templates/processors are found and reviewed. unknown means missing/unreviewed dynamic content or a non-pool structure with potentially code-defined behavior.',
        'template_resource_count':len(templates),'template_pool_resource_count':len(resources['template_pool']),
        'processor_list_resource_count':len(resources['processor_list']),'source_sha256':dict(sorted(source_hashes.items())),
    },
    'counts':{'structures':len(records),'loot':sum(x['status']=='loot' for x in records),'no_loot':sum(x['status']=='no_loot' for x in records),'unknown':sum(x['status']=='unknown' for x in records)},
    'structures':{row['id']:row for row in records},
}
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps(out,indent=2,sort_keys=True)+'\n')
runtime = {
    'format':1,
    'structures':{
        row['id']:{
            'status':row['status'],
            'has_containers':row['has_containers'],
            'container_blocks':row['container_blocks'],
            'loot_tables':row['loot_tables'],
            'unknowns':row['unknowns'],
            'template_count':row['reachable_template_count'],
        }
        for row in records
    }
}
args.output.with_name('structure-loot-runtime.json').write_text(json.dumps(runtime,indent=2,sort_keys=True)+'\n')
print('structures',out['counts'],'pool resources',len(resources['template_pool']),'template resources',len(resources['template']),'processor lists',len(resources['processor_list']))
print('unknown structures',', '.join(row['id'] for row in records if row['status']=='unknown'))
