#!/usr/bin/env python3
"""Extract reachable installed loot tables for structure previews; never executes pack code."""
from __future__ import annotations
import argparse, hashlib, json, re, zipfile
from pathlib import Path


def resource_id_from_path(name: str):
    name=name.replace('\\','/')
    bits=name.split('/')
    try:
        i=bits.index('loot_tables')
        if i < 1 or not name.endswith('.json'): return None
        return bits[i-1]+':'+ '/'.join(bits[i+1:])[:-5]
    except ValueError:
        return None


def walk_refs(node):
    if isinstance(node,dict):
        if node.get('type') in ('minecraft:loot_table','loot_table'):
            name=node.get('name') or node.get('value')
            if isinstance(name,str): yield name if ':' in name else 'minecraft:'+name
        for v in node.values(): yield from walk_refs(v)
    elif isinstance(node,list):
        for v in node: yield from walk_refs(v)


def display_path(path: Path, game: Path):
    try: return path.relative_to(game).as_posix()
    except ValueError:
        try: return '../'+path.relative_to(game.parent).as_posix()
        except ValueError: return path.name


def discover_sources(game: Path, client_jar: Path | None):
    sources=[]
    # Explicit datapacks precede packaged mod resources in effective priority.
    for root in (game/'kubejs'/'data', game/'global_packs'/'required_data', game/'datapacks'):
        if not root.exists(): continue
        for f in root.rglob('*.json'):
            rid=resource_id_from_path(f.relative_to(root).as_posix())
            if rid:
                try: raw=f.read_bytes()
                except OSError: continue
                sources.append((rid, 100, display_path(f,game), raw))
    # MC client and mod jars are immutable read-only inputs. KubeJS JSON has higher priority.
    jars=[]
    for base in (game.parent/'versions', game/'mods'):
        if base.exists(): jars += sorted(base.rglob('*.jar'))
    for jar in jars:
        try:
            with zipfile.ZipFile(jar) as z:
                for name in z.namelist():
                    rid=resource_id_from_path(name)
                    if rid:
                        try: raw=z.read(name)
                        except (KeyError, OSError, zipfile.BadZipFile): continue
                        # Vanilla first; mod resources have a deterministic stable order.
                        pri=0 if 'versions/' in str(jar) else 10
                        sources.append((rid, pri, f'{display_path(jar,game)}!/{name}', raw))
        except (OSError, zipfile.BadZipFile): pass
    if client_jar and client_jar.is_file():
        try:
            with zipfile.ZipFile(client_jar) as z:
                for name in z.namelist():
                    rid=resource_id_from_path(name)
                    if rid:
                        try: raw=z.read(name)
                        except (KeyError, OSError, zipfile.BadZipFile): continue
                        sources.append((rid,0,f'{display_path(client_jar,game)}!/{name}',raw))
        except (OSError, zipfile.BadZipFile): pass
    return sources


def extract_vase_modifier(game: Path):
    source=game/'kubejs/server_scripts/tfg/loot.js'
    try: raw=source.read_bytes(); text=raw.decode('utf-8')
    except (OSError,UnicodeDecodeError): return None
    start=text.find('//#region Vase Loot'); end=text.find('//#region Blocks',start)
    if start<0 or end<0: return None
    region=text[start:end]
    rolls=[tuple(map(int,m)) for m in re.findall(r'pool\.rolls\(\[\s*(\d+)\s*,\s*(\d+)\s*\]\)',region)]
    entries=[]
    for m in re.finditer(r"LootEntry\.of\(\s*['\"]([^'\"]+)['\"](?:\s*,\s*(\d+))?\s*\)",region):
        # Conditions are intentionally not evaluated. This records conservative item possibilities.
        item=m.group(1); count=int(m.group(2) or 1)
        entries.append({'id':item,'min':count,'max':count})
    return {
        'source':display_path(source,game),
        'sha256':hashlib.sha256(raw).hexdigest(),
        'block_pattern':'tfg:decorative_vase/generated/*',
        'pools':[{'rolls_min':a,'rolls_max':b} for a,b in rolls],
        'possible_drops':sorted({(x['id'],x['min'],x['max']) for x in entries}),
        'notes':['Generated-vase LootJS modifier replaces the normal block drops.',
                 'LootEntry conditions including randomChance are not evaluated; all listed items are possibilities, not guaranteed contents.',
                 'The modifier declares three independent pools with variable roll counts; item frequencies are not simulated.'],
    }


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--minecraft-dir',type=Path,required=True)
    p.add_argument('--structure-loot-runtime',type=Path,required=True)
    p.add_argument('--minecraft-client-jar',type=Path,help='Optional official 1.20.1 client jar, needed for vanilla structure tables.')
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args()
    base=json.loads(a.structure_loot_runtime.read_text())
    roots=set()
    for row in base.get('structures',{}).values(): roots.update(x for x in row.get('loot_tables',[]) if x)
    sources=discover_sources(a.minecraft_dir,a.minecraft_client_jar)
    by_id={}
    for rid,pri,src,raw in sources: by_id.setdefault(rid,[]).append((pri,src,raw))
    chosen={}
    for rid,items in by_id.items():
        items.sort(key=lambda x:(x[0],x[1]))
        pri=items[-1][0]
        top=[x for x in items if x[0]==pri]
        # When equal-priority resources collide, retain no arbitrary answer.
        hashes={hashlib.sha256(x[2]).hexdigest() for x in top}
        if len(hashes)>1:
            chosen[rid]={'ambiguous_sources':[x[1] for x in top], 'priority':pri}
        else:
            _,src,raw=top[-1]
            try: body=json.loads(raw)
            except (json.JSONDecodeError, UnicodeDecodeError):
                chosen[rid]={'source':src,'priority':pri,'invalid_json':True}
            else:
                chosen[rid]={'source':src,'priority':pri,'sha256':hashlib.sha256(raw).hexdigest(),'table':body}
    pending=list(sorted(roots)); reached=set(); missing={}
    while pending:
        rid=pending.pop()
        if rid in reached: continue
        reached.add(rid)
        row=chosen.get(rid)
        if row is None: missing[rid]='not found in installed resource inputs'; continue
        if 'table' not in row:
            missing[rid]='resource ambiguous/invalid'; continue
        for ref in walk_refs(row['table']):
            if ref not in reached: pending.append(ref)
    vase=extract_vase_modifier(a.minecraft_dir)
    if vase:
        vase['possible_drops']=[{'id':i,'min':lo,'max':hi} for i,lo,hi in vase['possible_drops']]
    out={
      'format':1,
      'provenance':{
        'description':'Reachable structure loot tables extracted read-only from installed Minecraft/mod resources and KubeJS datapacks. KubeJS takes precedence over jar resources; same-priority conflicts are marked unresolved.',
        'root_count':len(roots), 'resolved_count':sum('table' in chosen.get(k,{}) for k in reached),
        'missing_or_ambiguous':missing,
        'client_jar_supplied':bool(a.minecraft_client_jar),
      },
      'tables':{rid:chosen[rid] for rid in sorted(reached) if rid in chosen},
      'block_loot':{'generated_vase':vase} if vase else {},
    }
    a.output.parent.mkdir(parents=True,exist_ok=True)
    a.output.write_text(json.dumps(out,sort_keys=True,separators=(',',':'))+'\n')
    print(f"wrote {len(out['tables'])} reachable table definitions ({len(missing)} unresolved) to {a.output}")

if __name__=='__main__': main()
