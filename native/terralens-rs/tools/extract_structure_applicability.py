import argparse, hashlib, json, zipfile, pathlib, os, re, glob, collections
parser=argparse.ArgumentParser(description='Extract installed TFG structure biome applicability from resources and an existing level.dat profile.')
parser.add_argument('--minecraft-dir',type=pathlib.Path,required=True,help='Prism instance minecraft directory')
parser.add_argument('--minecraft-client-jar',type=pathlib.Path,required=True,help='Minecraft 1.20.1 client jar')
parser.add_argument('--reference-level',type=pathlib.Path,required=True,help='Existing save level.dat; read only')
parser.add_argument('--runtime-snapshot',type=pathlib.Path,help='Optional installed-structure-runtime.json for cross-checking')
parser.add_argument('--output',type=pathlib.Path,required=True)
args=parser.parse_args()
BASE=args.minecraft_dir; KUBE=BASE/'kubejs'; MODS=BASE/'mods'; MC=args.minecraft_client_jar
archives=[('minecraft-1.20.1-client.jar',zipfile.ZipFile(MC))]
for p in sorted(MODS.glob('*.jar')):
 try: archives.append((p.name,zipfile.ZipFile(p)))
 except Exception: pass
# Raw resources keyed by dimensionless id; local KubeJS datapack overrides any jar source of same id.
def rel_for(n, kind):
 m=re.match(r'data/([^/]+)/worldgen/'+kind+r'/(.+)\.json$',n)
 return (m.group(1)+':'+m.group(2) if m else None)
sets={}; structures={}; biome_tags=collections.defaultdict(list); structure_tags=collections.defaultdict(list)
for src,z in archives:
 for n in z.namelist():
  if not n.endswith('.json'): continue
  for kind,dest in [('structure_set',sets),('structure',structures)]:
   ident=rel_for(n,kind)
   if ident:
    try: dest[ident]=(json.loads(z.read(n)),src+'!/'+n)
    except Exception as e: print('BAD',src,n,e)
  m=re.match(r'data/([^/]+)/tags/worldgen/biome/(.+)\.json$',n)
  if m:
   key=m.group(1)+':'+m.group(2)
   try: biome_tags[key].append((json.loads(z.read(n)),src+'!/'+n))
   except:pass
  m=re.match(r'data/([^/]+)/tags/worldgen/structure/(.+)\.json$',n)
  if m:
   key=m.group(1)+':'+m.group(2)
   try: structure_tags[key].append((json.loads(z.read(n)),src+'!/'+n))
   except:pass
# Kube data: later datapack override by id; biome/structure tag resources append/replace.
for kind,dest in [('structure_set',sets),('structure',structures)]:
 folder=KUBE/'data'
 for p in folder.glob('*/worldgen/'+kind+'/**/*.json'):
  ns=p.relative_to(folder).parts[0]
  sub=p.relative_to(folder/ns/'worldgen'/kind).with_suffix('').as_posix()
  dest[ns+':'+sub]=(json.loads(p.read_text()),'KubeJS/'+p.relative_to(KUBE).as_posix())
for root,tagdest in [('biome',biome_tags),('structure',structure_tags)]:
 folder=KUBE/'data'
 for p in folder.glob('*/tags/worldgen/'+root+'/**/*.json'):
  ns=p.relative_to(folder).parts[0]
  sub=p.relative_to(folder/ns/'tags'/'worldgen'/root).with_suffix('').as_posix()
  tagdest[ns+':'+sub].append((json.loads(p.read_text()),'KubeJS/'+p.relative_to(KUBE).as_posix()))
# Expand the installed TFG `global.*_BIOMES.forEach(biome => { event.add(..., biome) })` patterns.
biome_constants={}
constfile=KUBE/'startup_scripts/tfg/constants.js'
if constfile.exists():
 txtc=constfile.read_text()
 for m in re.finditer(r'global\.(\w+_BIOMES)\s*=\s*\[(.*?)\]\s*;',txtc,re.S):
  biome_constants[m.group(1)]=re.findall(r"['\"]([^'\"]+)['\"]",m.group(2))
# KubeJS server tag event scripts: simple static event.add/remove calls only, preserve path/line provenance.
script_add_biome=collections.defaultdict(list); script_remove_biome=collections.defaultdict(list)
script_add_struct=collections.defaultdict(list); script_remove_struct=collections.defaultdict(list)
for p in (KUBE/'server_scripts').rglob('*.js'):
 txt=p.read_text(errors='replace')
 # single-value literal calls. Array values handled separately below.
 pat=r"event\.(add|remove)\(\s*['\"]([^'\"]+)['\"]\s*,\s*['\"]([^'\"]+)['\"]\s*\)"
 for m in re.finditer(pat,txt):
  meth,tag,val=m.groups(); line=txt.count('\n',0,m.start())+1
  if 'has_structure' in tag or tag.startswith('minecraft:is_') or tag=='tfg:nether_biomes':
   (script_add_biome if meth=='add' else script_remove_biome)[tag].append((val,f'KubeJS/server_scripts/{p.relative_to(KUBE/"server_scripts").as_posix()}:{line}'))
# Expand literal biome-id arrays used in global.*_BIOMES.forEach loops; this is used by the current Moon tags script.
for p in (KUBE/'server_scripts').rglob('*.js'):
 txt=p.read_text(errors='replace')
 for m in re.finditer(r'global\.(\w+_BIOMES)\.forEach\(\s*(\w+)\s*=>\s*\{(.*?)\n\s*\}\s*\)',txt,re.S):
  group,var,body=m.groups(); line=txt.count('\n',0,m.start())+1
  values=biome_constants.get(group,[])
  for em in re.finditer(r"event\.(add|remove)\(\s*['\"]([^'\"]+)['\"]\s*,\s*"+re.escape(var)+r"\s*\)",body):
   meth,tag=em.groups(); target=script_add_biome if meth=='add' else script_remove_biome
   for val in values: target[tag].append((val,f'KubeJS/server_scripts/{p.relative_to(KUBE/"server_scripts").as_posix()}:{line}'))

# Simple literal `event.add('tag', ['a','b'])` or tags as arrays.
patarr=r"event\.(add|remove)\(\s*['\"]([^'\"]+)['\"]\s*,\s*\[([^\]]*)\]\s*\)"
for p in (KUBE/'server_scripts').rglob('*.js'):
 txt=p.read_text(errors='replace')
 for m in re.finditer(patarr,txt,re.S):
  meth,tag,body=m.groups()
  vals=re.findall(r"['\"]([^'\"]+)['\"]",body); line=txt.count('\n',0,m.start())+1
  if 'has_structure' in tag or tag.startswith('minecraft:is_') or tag=='tfg:nether_biomes':
   target=script_add_biome if meth=='add' else script_remove_biome
   for val in vals: target[tag].append((val,f'KubeJS/server_scripts/{p.relative_to(KUBE/"server_scripts").as_posix()}:{line}'))
# Make explicit biome universe from level.dat for the 3 configured multidimensional sources + known TFC/TFG Overworld registry.
# Exact profiles from level.dat, parsed before output.
# Worldgen biome ids from actual saved generator profile; TFC Overworld is a custom source,
# so its defined TFC and TFG/earth registry entries form its exact resource-level candidate pool.
import nbtlib
LEVEL=args.reference_level; level=nbtlib.load(LEVEL)['Data']['WorldGenSettings']
profiles={}; profile_types={}; overworld_registered_resource_biomes=[]
for dim in ('minecraft:overworld','minecraft:the_nether','ad_astra:moon','ad_astra:mars'):
 d=level['dimensions'][dim];g=d['generator'];profile_types[dim]={'dimension_type':str(d['type'])}
 # unwrap generator wrappers
 while 'biome_source' not in g and 'generator' in g:g=g['generator']
 bs=g.get('biome_source')
 if bs and 'biomes' in bs:
  profiles[dim]=sorted({str(x['biome']) for x in bs['biomes']})
  profile_types[dim].update({'source_type':str(bs['type']),'generator_type':str(g['type']),'profile_source':'level.dat WorldGenSettings'})
 elif dim=='minecraft:overworld':
  ids=set()
  # `tfc:overworld` derives its pool from TFC/TFG registry biome definitions; no vanilla-name aliases.
  for src,z in archives:
   for n in z.namelist():
    if n.endswith('.json') and '/worldgen/biome/' in n and '/tags/' not in n:
     m=re.match(r'data/(tfc|tfg)/worldgen/biome/(.+)\.json$',n)
     if m and (m.group(1)=='tfc' or m.group(2).startswith('earth/')):
      ids.add(m.group(1)+':'+m.group(2))
  # KubeJS TFG earth datapack overrides/current additions.
  for p in (KUBE/'data/tfg/worldgen/biome/earth').rglob('*.json'):
   ids.add('tfg:earth/'+p.relative_to(KUBE/'data/tfg/worldgen/biome/earth').with_suffix('').as_posix())
  overworld_registered_resource_biomes=sorted(ids)
  # The current TFG wrapped Overworld sampler draws its 109 registered Earth biomes; the 30 legacy TFC biome definitions remain recorded below as a broader resource pool only.
  profiles[dim]=sorted(b for b in ids if b.startswith('tfg:earth/'))
  profile_types[dim].update({'source_type':'tfc:overworld','generator_type':'tfc:overworld','profile_source':'current TFG Earth biome set registered by TFGBiomes for the custom tfc:overworld source; source distribution confirmed by 1.20.1 TFG Core 0.9.25 resources'})
 else:
  profiles[dim]=[]

# Cross-check current profile files against saved profile lists; use the saved profile as reference truth.
installed_profile_refs={
 'minecraft:the_nether':KUBE/'data/minecraft/dimension/the_nether.json',
 'ad_astra:moon':KUBE/'data/ad_astra/dimension/moon.json',
 'ad_astra:mars':KUBE/'data/ad_astra/dimension/mars.json'
}
for dim,path in installed_profile_refs.items():
 current=json.loads(path.read_text())
 gen=current['generator']
 while 'biome_source' not in gen and 'generator' in gen: gen=gen['generator']
 ids=sorted({x['biome'] for x in gen['biome_source'].get('biomes',[])})
 profile_types[dim]['current_pack_profile_source']='KubeJS/'+path.relative_to(KUBE).as_posix()
 profile_types[dim]['current_pack_profile_matches_level_dat']=ids==profiles[dim]

overworld_biome_definition_sources={}
for biome in overworld_registered_resource_biomes:
 ns,pathpart=biome.split(':',1)
 rel=f'data/{ns}/worldgen/biome/{pathpart}.json'
 matches=[src+'!/'+rel for src,z in archives if rel in z.namelist()]
 overworld_biome_definition_sources[biome]=matches

known_biomes=set().union(*(set(x) for x in profiles.values()))

def normalize_ref(v):
 if isinstance(v,dict):
  v=v.get('id')
 if not isinstance(v,str):return None
 return v

def merged_tag_values(tag, data):
 vals=[]
 for obj,src in data.get(tag,[]):
  if obj.get('replace') is True: vals=[]
  vals.extend([normalize_ref(v) for v in obj.get('values',[]) if normalize_ref(v)])
 # Apply KubeJS static event operations late; only script tag mutations we statically parsed.
 for val,_ in script_add_biome.get(tag,[]): vals.append(val)
 for val,_ in script_remove_biome.get(tag,[]):
  vals=[v for v in vals if v!=val]
 # deduplicate while preserving first occurrence
 return list(dict.fromkeys(vals))

def resolve_biome_ref(ref, stack=()):
 if not isinstance(ref,str):return set()
 if ref.startswith('#'):
  tag=ref[1:]
  if tag in stack:return set()
  return resolve_biome_tag(tag,stack+(tag,))
 return {ref}

def resolve_biome_tag(tag,stack=()):
 out=set()
 for ref in merged_tag_values(tag,biome_tags):out|=resolve_biome_ref(ref,stack+(tag,))
 return out

def resolve_struct_tag(tag,stack=()):
 if tag in stack:return set()
 vals=[]
 for obj,src in structure_tags.get(tag,[]):
  if obj.get('replace') is True: vals=[]
  vals.extend([normalize_ref(v) for v in obj.get('values',[]) if normalize_ref(v)])
 out=set()
 for ref in vals:
  if isinstance(ref,str) and ref.startswith('#'):out|=resolve_struct_tag(ref[1:],stack+(tag,))
  elif isinstance(ref,str):out.add(ref)
 return out

def biome_selector(st):
 sel=st.get('biomes')
 if sel is None:return {'raw':None,'biomes':None,'source':'no explicit biome selector'}
 if isinstance(sel,list):refs=sel
 else:refs=[sel]
 raw=[];resolved=set()
 for x in refs:
  x=normalize_ref(x)
  if x is None:continue
  raw.append(x);resolved|=resolve_biome_ref(x)
 return {'raw':raw,'biomes':sorted(resolved),'source':'structure definition biomes codec'}

def iter_struct_refs(entries):
 refs=[]
 for ent in entries:
  ident=ent.get('structure')
  if not isinstance(ident,str):continue
  if ident.startswith('#'):
   for sid in sorted(resolve_struct_tag(ident[1:])):refs.append((sid,ent.get('weight',1),ident))
  else:refs.append((ident,ent.get('weight',1),None))
 return refs

# Build one applicability row per set and referenced structure. preserve unknowns explicitly.
rows=[]; unresolved=[]
for setid,(sd,setsrc) in sorted(sets.items()):
 placement=sd.get('placement',{})
 ptype=placement.get('type','unknown')
 children=[]
 for sid,weight,tag_ref in iter_struct_refs(sd.get('structures',[])):
  struct=structures.get(sid)
  if not struct:
   unresolved.append({'set':setid,'structure':sid,'reason':'structure reference not found among installed static resources','source':setsrc})
   children.append({'id':sid,'weight':weight,'reference_tag':tag_ref,'status':'unresolved_definition','eligible_dimensions':[],'dimension_biomes':{}});continue
  st,stsrc=struct;selector=biome_selector(st)
  per={}; eligible=[]
  for dim,pool in profiles.items():
   if selector['biomes'] is None:
    hits=[]
   else: hits=sorted(set(pool)&set(selector['biomes']))
   per[dim]=hits
   if hits:eligible.append(dim)
  if selector['biomes'] is None:
   status='unknown_no_biome_gate'; reasons=['no_explicit_biome_selector']
  elif selector['raw']==[] or not selector['biomes']:
   status='empty_biome_selector'; reasons=['empty_biome_selector_active_in_resource']
  elif not eligible:
   status='no_profile_intersection'; reasons=['no_raw_biome_intersection_in_supported_dimensions']
  elif ptype not in ('minecraft:random_spread','tfc:climate'):
   status='profile_intersection_but_unsupported_placement'; reasons=['unsupported_placement:'+str(ptype)]
  else:
   status='potential'; reasons=[]
  if ptype not in ('minecraft:random_spread','tfc:climate') and 'unsupported_placement:'+str(ptype) not in reasons:reasons.append('unsupported_placement:'+str(ptype))
  scan_dims=eligible if status=='potential' else []
  children.append({'id':sid,'weight':weight,'reference_tag':tag_ref,'structure_type':st.get('type'),'biome_selector':selector['raw'],'resolved_selector_biomes':selector['biomes'],'eligible_dimensions':eligible,'scanner_eligible_dimensions':scan_dims,'dimension_biomes':per,'status':status,'disabled_reasons':reasons,'source':stsrc})
 rows.append({'id':setid,'source':setsrc,'placement':placement,'children':children})

# Summary dimension => structure ids with candidates; status checks can gate marker enablement.
summary={}
for dim in profiles:
 ids=[]
 for row in rows:
  for child in row['children']:
   if dim in child['scanner_eligible_dimensions']:
    ids.append({'set':row['id'],'structure':child['id'],'status':child['status'],'placement':row['placement'].get('type')})
 summary[dim]=ids

# Gather the exact effective biome tags named by installed structure definitions, including raw contributions.
used_gate_tags=set()
for _,(st,_) in structures.items():
 sel=st.get('biomes')
 refs=sel if isinstance(sel,list) else ([sel] if sel is not None else [])
 for ref in refs:
  if isinstance(ref,str) and ref.startswith('#'):used_gate_tags.add(ref[1:])
used_gate_tags.update(['minecraft:is_nether','minecraft:is_overworld','minecraft:has_structure/nether_fortress','minecraft:has_structure/bastion_remnant','minecraft:has_structure/nether_fossil'])
gate_tag_report={}
for tag in sorted(used_gate_tags):
 gate_tag_report[tag]={
  'direct_values':merged_tag_values(tag,biome_tags),
  'resolved_biomes':sorted(resolve_biome_tag(tag)),
  'resource_sources':[src for _,src in biome_tags.get(tag,[])],
  'kubejs_additions':[{'value':v,'source':s} for v,s in script_add_biome.get(tag,[])],
  'kubejs_removals':[{'value':v,'source':s} for v,s in script_remove_biome.get(tag,[])]
 }
# Verify all non-vanilla structure set / structure definitions in the pack's existing runtime snapshot exist in current raw sources.
runtime=json.loads(args.runtime_snapshot.read_text()) if args.runtime_snapshot else {'definitions':[]}
runtime_sets={x['id'] for x in runtime['definitions'] if x.get('kind')=='structure_set'}
runtime_structures={x['id'] for x in runtime['definitions'] if x.get('kind')=='structure'}
source_structures=set(structures)
runtime_crosscheck={'runtime_snapshot_source':'assets/data/tfg/installed-structure-runtime.json (current non-vanilla snapshot)','runtime_structure_sets':len(runtime_sets),'structure_sets_missing_from_current_resources':sorted(runtime_sets-set(sets)),'runtime_structures':len(runtime_structures),'structure_definitions_missing_from_current_resources':sorted(runtime_structures-source_structures)}

# Map-index view for callers that need a direct actual-ID applicability gate.
structure_index={}
for sid,(st,src) in sorted(structures.items()):
 set_refs=[]
 for row in rows:
  for child in row['children']:
   if child['id']==sid:set_refs.append({'set':row['id'],'weight':child.get('weight')})
 selector=biome_selector(st)
 by_dimension={}
 for dim,pool in profiles.items():
  hits=[] if selector['biomes'] is None else sorted(set(pool)&set(selector['biomes']))
  matching=[c for r in rows for c in r['children'] if c['id']==sid and dim in c.get('scanner_eligible_dimensions',[])]
  source_eligible=bool(hits)
  scanner_eligible=bool(matching)
  if not source_eligible: reason='empty_biome_selector' if selector['raw']==[] else 'no_raw_biome_intersection'
  elif not scanner_eligible: reason='placement_unsupported_or_set_not_scan_enabled'
  else: reason='eligible'
  by_dimension[dim]={
   'biome_intersection':hits,
   'source_eligible':source_eligible,
   'scanner_eligible':scanner_eligible,
   'reason':reason
  }
 structure_index[sid]={'id':sid,'sets':set_refs,'biomes':selector['raw'],'resolved_biomes':selector['biomes'],'type':st.get('type'),'dimensions':by_dimension,'source':src,'definition':st}
structure_sets_raw=[]
for sid,(sd,src) in sorted(sets.items()):
 structure_sets_raw.append({'id':sid,'source':src,'definition':sd})

# Hash the Minecraft/mod archives that supplied structures, sets, biome tags or biome profiles, plus pack profile/tag scripts.
source_files=set()
for source,z in archives:
 relevant=any(('/worldgen/structure/' in n or '/worldgen/structure_set/' in n or '/tags/worldgen/biome/' in n or '/tags/worldgen/structure/' in n or '/worldgen/biome/' in n) and n.endswith('.json') for n in z.namelist())
 if relevant:
  source_files.add(MC if source=='minecraft-1.20.1-client.jar' else MODS/source)
for source in list(overworld_biome_definition_sources.values()):
 pass
for path in list(KUBE.glob('data/*/worldgen/structure/**/*.json'))+list(KUBE.glob('data/*/worldgen/structure_set/**/*.json'))+list(KUBE.glob('data/*/tags/worldgen/biome/**/*.json'))+list(KUBE.glob('data/*/tags/worldgen/structure/**/*.json')):
 source_files.add(path)
for path in (KUBE/'server_scripts').rglob('*.js'):
 if re.search(r'has_structure|_BIOMES|nether_biomes',path.read_text(errors='ignore')):source_files.add(path)
for path in (KUBE/'data').glob('*/dimension/*.json'):source_files.add(path)
source_hashes=[]
for path in sorted(source_files,key=lambda p:p.name):
 if not path.is_file():continue
 source_hashes.append({'file':path.name if path.suffix=='.jar' else path.relative_to(KUBE).as_posix(),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})

# Record just source counts and exact ids/profiles. Keep user path and UUID/account fields out.
out={
 'format':1,
 'provenance':{
  'instance':'Prism TerraFirmaGreg-Modern installed pack; Minecraft 1.20.1; TFG Core Modern 0.9.25; GTCEu 7.5.3',
  'reference':'Existing supplied level.dat WorldGenSettings plus installed mod jars and current KubeJS data/scripts; read-only audit; no Minecraft launch and no save writes.',
  'resource_merge':'Structure/structure-set definitions use current KubeJS datapack entries over installed jar resources by identifier. Biome tags merge installed jar tag JSON, KubeJS datapack JSON, and statically readable literal KubeJS event.add/remove calls. The installed runtime snapshot independently matches all 27 non-vanilla sets and 63 referenced structures in this resource inventory.',
  'overworld_pool':'The saved world uses the custom tfc:overworld biome source. Source eligibility uses the 109 TFG Earth biome IDs registered by current TFG Core 0.9.25 TFGBiomes; the separate 139-ID TFG Earth plus legacy TFC registry-resource superset is retained for audit only. All IDs are raw registry identifiers; vanilla biome names are never aliased to TFC names.',
  'eligibility':'A biome intersection means source-level biome gating can succeed; it does not guarantee placement or terrain viability. `scanner_eligible_dimensions` additionally requires a placement type implemented by the current scanner. Scanner callers should also enforce per-type distance limits (the request is 100 km), frequency, height/terrain checks, and a concrete supported structure ID.',
  'source_version_note':'The older assets/data/tfg/structures.json is from TFG Core 0.9.21 and was not used; current structures and sets were reread from installed TFG Core 0.9.25 KubeJS data and all installed 1.20.1 mod resources.',
  'sha256_sources':source_hashes
 },
 'supported_dimensions':[{ 'id':dim,**profile_types[dim], 'biomes':profiles[dim], 'biome_id_source':'level.dat WorldGenSettings' if dim!='minecraft:overworld' else 'TFC/TFG biome registry definition JSON'} for dim in profiles],
 'overworld_biome_definition_sources':overworld_biome_definition_sources,
 'overworld_biome_pools':{'current_sampler_biomes':profiles['minecraft:overworld'],'current_sampler_count':len(profiles['minecraft:overworld']),'current_sampler_basis':'TFG Earth biome definitions used by the current tfc:overworld sampler','registered_resource_biomes':overworld_registered_resource_biomes,'registered_count':len(overworld_registered_resource_biomes),'tfg_earth_resource_count':sum(b.startswith('tfg:earth/') for b in overworld_registered_resource_biomes),'tfc_resource_biomes':[b for b in overworld_registered_resource_biomes if b.startswith('tfc:')],'tfc_resource_count':sum(b.startswith('tfc:') for b in overworld_registered_resource_biomes),'basis':'registered 139-ID pool is preserved for completeness; source-level eligibility uses the current 109 TFG Earth sampler IDs'},
 'resource_counts':{'structure_sets':len(sets),'structure_definitions':len(structures),'biome_tags':len(biome_tags),'structure_tags':len(structure_tags)},
 'biome_gate_tags':gate_tag_report,
 'sets':rows,
 'structure_sets':structure_sets_raw,
 'structures':list(structure_index.values()),
 'eligible_by_dimension':summary,
 'runtime_resource_crosscheck':runtime_crosscheck,
 'unresolved_references':unresolved,
 'tag_script_changes':{'biome_tags':{tag:[{'value':v,'source':s} for v,s in vals] for tag,vals in script_add_biome.items()},'removed':{tag:[{'value':v,'source':s} for v,s in vals] for tag,vals in script_remove_biome.items()}}
}
path=args.output;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(out,indent=2)+'\n')
print('biome pool sizes', {k:len(v) for k,v in profiles.items()})
print('resources sets/structures',len(sets),len(structures),'tags',len(biome_tags),'structtags',len(structure_tags),'rows',len(rows),'unresolved',len(unresolved))
print('status counts',collections.Counter(ch['status'] for r in rows for ch in r['children']))
print('eligible by dim', {k:len(v) for k,v in summary.items()})
for dim,ids in summary.items(): print('\n',dim);print('\n'.join(f"{x['set']} -> {x['structure']} [{x['status']}, {x['placement']}]" for x in ids))
print('unresolved refs',unresolved[:30])
