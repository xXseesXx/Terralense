#!/usr/bin/env python3
"""Rebuild a deterministic Minecraft 1.20 FeatureSorter profile from installed TFG biome/tag JSON.

This is a research/development extraction script, not part of runtime. It intentionally sorts
possible biome keys to produce a reproducible offline profile. TFG's live IdentityHashMap order
is JVM-dependent, so the result is not claimed to reproduce a particular game process.
"""
from __future__ import annotations
import argparse, hashlib, json, re, zipfile
from collections import defaultdict
from pathlib import Path

INSTANCE = Path.home()/'.local/share/PrismLauncher/instances/TerraFirmaGreg-Modern/minecraft'
PROJECT = Path(__file__).resolve().parents[1]
MODS = INSTANCE/'mods'
CORE_JAR = MODS/'TerraFirmaGreg-Core-Modern-0.9.25.jar'
TFC_SOURCE = Path('/tmp/tfg-core-0.9.25/src/main/resources')


def read_resource(pack: Path, name: str):
    if pack.is_dir():
        p=pack/name
        return p.read_bytes() if p.exists() else None
    try:
        with zipfile.ZipFile(pack) as z:
            return z.read(name) if name in z.namelist() else None
    except (OSError, zipfile.BadZipFile):
        return None


def resource_names(pack: Path, prefix: str):
    if pack.is_dir():
        return [str(p.relative_to(pack)) for p in pack.rglob('*.json') if str(p.relative_to(pack)).startswith(prefix)]
    try:
        with zipfile.ZipFile(pack) as z:
            return [n for n in z.namelist() if n.startswith(prefix) and n.endswith('.json')]
    except (OSError, zipfile.BadZipFile):
        return []


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--out', type=Path, default=PROJECT/'native/terralens-rs/src/vegetation/assets/feature-order.json')
    ap.add_argument('--instance', type=Path, default=INSTANCE)
    args=ap.parse_args()
    mods=args.instance/'mods'
    core=mods/'TerraFirmaGreg-Core-Modern-0.9.25.jar'
    if not core.exists(): raise SystemExit(f'missing expected installed jar: {core}')
    # TFG's extension registry possibleBiomes sorted to create the canonical offline profile.
    with zipfile.ZipFile(core) as z:
        biome_keys=sorted('tfg:earth/'+n.split('/earth/',1)[1][:-5] for n in z.namelist()
                          if n.startswith('data/tfg/worldgen/biome/earth/') and n.endswith('.json'))
        biome_json={key:json.loads(z.read('data/tfg/worldgen/biome/earth/'+key.split('tfg:earth/',1)[1]+'.json'))
                    for key in biome_keys}
    # Resource-pack order for tags is approximated as lexicographic mod-jar order; core's source
    # directory is included only if a corresponding resource is absent in the jar.
    packs=sorted(mods.glob('*.jar'), key=lambda p:p.name)
    tags={}
    marker='/tags/worldgen/placed_feature/'
    for pack in packs:
        if pack.is_dir():
            pairs=((str(p.relative_to(pack)), p.read_bytes()) for p in pack.rglob('*.json')
                   if marker in str(p.relative_to(pack)))
        else:
            try:
                z=zipfile.ZipFile(pack)
                pairs=((name,z.read(name)) for name in z.namelist()
                       if marker in name and name.endswith('.json'))
            except (OSError, zipfile.BadZipFile):
                continue
        for name,raw in pairs:
            ident=name.split(marker,1)[1][:-5]
            ns=name.split('/')[1]
            key=ns+':'+ident
            obj=json.loads(raw)
            if obj.get('replace') or key not in tags: tags[key]=[]
            tags.setdefault(key,[]).extend(obj.get('values',[]))
    # KubeJS adds TFG crop IDs to data-driven placed-feature tags via the mod tag event.
    # Apply `event.add` in source order after mod-jar JSON tags, matching this instance's scripts.
    script_root=args.instance/'kubejs/server_scripts'
    if script_root.exists():
        for script in sorted(script_root.rglob('*.js')):
            whole=script.read_text(errors='replace')
            match=re.search(r'function registerTFGOverworldPlacedFeatures\(event\) \{(.*?)(?=\nfunction registerTFGOverworld|\Z)', whole, re.S)
            if not match: continue
            source=match.group(1)
            for m in re.finditer(r'event\.add\(\s*["\']([^"\']+)["\']\s*,\s*["\']([^"\']+)["\']\s*\)', source):
                tag,value=m.groups()
                if tag.startswith(('tfc:feature/','tfg:feature/','firmalife:feature/')) or (tag.startswith(('tfc:in_biome/','tfg:in_biome/')) and value in ('tfg:earth/crop/palm_tree/coconut_patch','tfg:earth/crop/palm_tree/date_patch')):
                    tags.setdefault(tag,[]).append(value)
            for m in re.finditer(r'event\.remove\(\s*["\']([^"\']+)["\']\s*,\s*["\']([^"\']+)["\']\s*\)', source):
                tag,value=m.groups()
                if tag in tags and tag.startswith(('tfc:feature/','tfg:feature/','firmalife:feature/')):
                    tags[tag]=[v for v in tags[tag] if v!=value]
            # The script uses a small array + forEach(biome => event.add(...)) for palm patches.
            palms=re.search(r'const palmFruitBiomes = \[([^]]*)\]', source, re.S)
            if palms:
                for tag in re.findall(r'[\"\']((?:tfc|tfg):in_biome/[^\"\']+)[\"\']', palms.group(1)):
                    tags.setdefault(tag,[]).extend(['tfg:earth/crop/palm_tree/coconut_patch','tfg:earth/crop/palm_tree/date_patch'])

    def expand_entry(entry, stack=()):
        optional=False
        if isinstance(entry,dict):
            val=entry.get('id','')
            optional=entry.get('required') is False
        else: val=entry
        if val.startswith('#'):
            tag=val[1:]
            if tag in stack: raise ValueError('cyclic placed feature tag: '+' -> '.join(stack+(tag,)))
            return expand_tag(tag,stack+(tag,))
        return [val]
    def expand_tag(tag,stack=()):
        vals=tags.get(tag)
        if vals is None: return []
        out=[]
        for ent in vals: out.extend(expand_entry(ent,stack))
        return out

    biome_steps={}
    max_steps=0
    for biome in biome_keys:
        steps=[]
        for feature_list in biome_json[biome].get('features',[]):
            flat=expand_entry(feature_list) if isinstance(feature_list,str) else []
            if isinstance(feature_list,list):
                for e in feature_list: flat.extend(expand_entry(e))
            steps.append(flat)
        biome_steps[biome]=steps
        max_steps=max(max_steps,len(steps))

    # Faithful FeatureSorter graph: global IDs by first feature encounter, edge each consecutive
    # placed feature within each biome and generation step, DFS roots/successors by (step,globalId).
    global_ids={}
    nodes=set()
    edges=defaultdict(set)
    for biome in biome_keys:
        for step, features in enumerate(biome_steps[biome]):
            seq=[]
            for fid in features:
                if fid not in global_ids: global_ids[fid]=len(global_ids)
                node=(step,global_ids[fid]); nodes.add(node); seq.append(node)
            for a,b in zip(seq,seq[1:]):
                if a!=b: edges[a].add(b)
    visited=set(); active=[]; active_set=set(); post=[]
    def visit(node):
        if node in visited:return
        if node in active_set: raise ValueError('cycle path '+repr([next((k for k,v in global_ids.items() if v==x[1]),'?') for x in active[active.index(node):]+[node]]))
        active.append(node); active_set.add(node)
        for nxt in sorted(edges[node]): visit(nxt)
        active.pop(); active_set.remove(node); visited.add(node); post.append(node)
    for n in sorted(nodes): visit(n)
    order=list(reversed(post))
    per_stage=defaultdict(list)
    for step,gid in order: per_stage[step].append(gid)
    stage_orders={str(s):[next(k for k,v in global_ids.items() if v==gid) for gid in gids]
                  for s,gids in sorted(per_stage.items())}
    feature_biomes=defaultdict(set)
    for biome, steps in biome_steps.items():
        for features in steps:
            for fid in features: feature_biomes[fid].add(biome)
    active_crops=[]
    crop_file=PROJECT/'native/terralens-rs/src/vegetation/assets/crop_features.json'
    if crop_file.exists():
        crop_ids={f['id'] for f in json.loads(crop_file.read_text()).get('features',[])}
        for step, ids in per_stage.items():
            active_crops.extend({'id':fid,'step':step,'index':i} for i,fid in enumerate(stage_orders[str(step)]) if fid in crop_ids)
    forest=[{'id':fid,'step':step,'index':i} for step,ids in per_stage.items()
            for i,fid in enumerate(stage_orders[str(step)]) if fid=='tfg:earth/forest']
    out={'schema':2,'game':'Minecraft 1.20.1 / Forge 47.4.13',
         'mods':{'tfc':'3.2.25','tfg_core_modern':'0.9.25'},
         'profile':{'id':'tfg-0.9.25-resourcekey-sorted-possible-biomes',
                    'biomeInputOrder':'lexicographic resource-key order (canonical offline reconstruction)',
                    'biomeCount':len(biome_keys),
                    'biomeKeysSha256':hashlib.sha256(('\n'.join(biome_keys)+'\n').encode()).hexdigest(),
                    'possibleBiomeOrder':biome_keys,
                    'tagResourcePackOrder':[p.name for p in packs]},
         'featureSorter':{'description':'MC FeatureSorter: first-seen feature IDs, per-biome/per-stage consecutive edges, DFS roots/successors ordered by (step,globalId), reverse postorder.',
                          'runtimeCaveat':'TFGBiomes.TFG_EXTENSIONS is an IdentityHashMap, so live possible-biome order can differ. Tag resources are merged in lexicographic mod-jar filename order; KubeJS processing includes literal feature-tag additions/removals and the expanded palm-fruit biome loop from registerTFGOverworldPlacedFeatures. Other dynamic KubeJS biome-tag additions are excluded because applying every script event as a raw tag list produced a cyclic graph; actual runtime ordering/semantics must be captured to match live indices. These indexes/memberships are exact for this documented canonical extraction profile, not a captured runtime profile.'},
         'stages':stage_orders,
         'featureBiomes':{fid:sorted(biomes) for fid,biomes in sorted(feature_biomes.items())},
         'activeCropFeatures':active_crops,
         'forestFeature':forest[0] if forest else None,
         'seedAlgorithm':{'base':'Helpers.hash(128739412341L, chunkOriginBlockPos)',
                           'hash':'h=salt XOR ((long)x*501125321) XOR ((long)y*1136930381) XOR z; h*=0x27d4eb2d; return sign-extended (int)h',
                           'feature':'random.setSeed(base); s=(index*random.nextLong()*203704237L) XOR (step*random.nextLong()*758031792L) XOR base; random.setSeed(s)',
                           'random':'WorldgenRandom wrapping XoroshiroRandomSource; same random passed through feature placement and reseeded per feature.'}}
    args.out.parent.mkdir(parents=True,exist_ok=True)
    args.out.write_text(json.dumps(out,indent=2)+'\n')
    print(f'wrote {args.out}; biomes={len(biome_keys)} features={len(global_ids)} stages={len(stage_orders)} crops={len(active_crops)} forest={forest[:1]}')

if __name__=='__main__': main()
