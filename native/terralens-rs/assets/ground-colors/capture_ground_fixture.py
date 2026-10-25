#!/usr/bin/env python3
"""Capture small ground references directly from World 0 Anvil/NBT records.

This deliberately uses an independent Python NBT and packed-state reader, not the
Rust Prism reader. Reads only existing chunks and writes a bounded fixture.
"""
import gzip
import hashlib
import json
import struct
import sys
import zlib
from pathlib import Path

WORLD = Path(sys.argv[1]).expanduser()
OUT = Path(sys.argv[2]).expanduser()
SITES = [
    {"name": "floodplain_nearshore", "chunk": [552, 694]},
    {"name": "active_shield_canopy", "chunk": [-173, 282]},
    {"name": "cinder_cone", "chunk": [-79, 293]},
    {"name": "glacial_tuya_snow", "chunk": [3122, 1764]},
    {"name": "tuya_raw_rock", "chunk": [344, -398]},
    {"name": "saved_plain", "chunk": [0, 0]},
]

class Reader:
    def __init__(self, data): self.data, self.p = data, 0
    def take(self, n):
        b = self.data[self.p:self.p+n]
        if len(b) != n: raise ValueError("truncated NBT")
        self.p += n; return b
    def u8(self): return self.take(1)[0]
    def i8(self): return struct.unpack('>b', self.take(1))[0]
    def u16(self): return struct.unpack('>H', self.take(2))[0]
    def i16(self): return struct.unpack('>h', self.take(2))[0]
    def i32(self): return struct.unpack('>i', self.take(4))[0]
    def i64(self): return struct.unpack('>q', self.take(8))[0]
    def string(self): return self.take(self.u16()).decode('utf8')
    def payload(self, tag):
        if tag == 1: return self.i8()
        if tag == 2: return self.i16()
        if tag == 3: return self.i32()
        if tag == 4: return self.i64()
        if tag == 5: return struct.unpack('>f', self.take(4))[0]
        if tag == 6: return struct.unpack('>d', self.take(8))[0]
        if tag == 7:
            n=self.i32(); return self.take(n) if n >= 0 else (_ for _ in ()).throw(ValueError("bad byte array"))
        if tag == 8: return self.string()
        if tag == 9:
            child=self.u8(); n=self.i32(); return [self.payload(child) for _ in range(n)]
        if tag == 10:
            d={}
            while True:
                child=self.u8()
                if child == 0: return d
                name=self.string()
                d[name] = self.payload(child)
        if tag == 11:
            n=self.i32(); return [self.i32() for _ in range(n)]
        if tag == 12:
            n=self.i32(); return [self.i64() for _ in range(n)]
        raise ValueError(f"unknown NBT tag {tag}")
    def root(self):
        tag=self.u8()
        if tag != 10: raise ValueError("chunk root is not compound")
        self.string(); return self.payload(tag)

def read_chunk(cx, cz):
    rx, rz = cx // 32, cz // 32
    path = WORLD / "region" / f"r.{rx}.{rz}.mca"
    with path.open('rb') as f:
        header=f.read(8192)
        slot=(cz % 32)*32+(cx % 32)
        entry=header[slot*4:slot*4+4]
        sector=int.from_bytes(entry[:3], 'big')
        if sector < 2 or entry[3] == 0: return None, path
        f.seek(sector*4096)
        n=struct.unpack('>I',f.read(4))[0]
        compression=f.read(1)[0]
        payload=f.read(n-1)
    raw={1:gzip.decompress,2:zlib.decompress,3:lambda x:x}[compression](payload)
    return Reader(raw).root(), path

def ground_material(name):
    ns, _, path=name.partition(':')
    if path.endswith(('water','lava','magma')) or '/magma/' in path or path in ('magma_block','hardened_clay'):
        return True
    if path.startswith(('grass/','clay_grass/','dirt/','coarse_dirt/','duff/','mud/','clay/','rooted_dirt/','sand/','rock/raw/','rock/gravel/','rock/hardened/')):
        return True
    if path == 'pile/volcanic_ash': return True
    return path in {'stone','deepslate','dirt','coarse_dirt','podzol','mycelium','grass_block','sand','red_sand','gravel','snow_block','ice','packed_ice','blue_ice','netherrack','basalt','blackstone','end_stone'}

def columns(root):
    data=root.get('Level',root)
    status=data.get('Status')
    if status not in ('minecraft:full','full'): return None
    sections=[]
    for sec in data.get('sections',data.get('Sections',[])):
        y=sec.get('Y'); states=sec.get('block_states')
        if states is not None: palette=states.get('palette',[]); packed=states.get('data',[])
        else: palette=sec.get('Palette',[]); packed=sec.get('BlockStates',[])
        names=[p.get('Name','') for p in palette]
        sections.append((y,names,packed))
    out=[]
    for z in range(16):
      for x in range(16):
        highest=ground=None
        for sy,names,packed in sorted(sections,key=lambda v:v[0],reverse=True):
          if not names: continue
          bits=max(4,(len(names)-1).bit_length()) if len(names)>1 else 0
          per=64//bits if bits else 0
          mask=(1<<bits)-1 if bits else 0
          for ly in range(15,-1,-1):
            index=(ly*16+z)*16+x
            pi=0 if len(names)==1 else ((packed[index//per] & ((1<<64)-1)) >> ((index%per)*bits)) & mask
            name=names[pi]
            if name.rsplit(':',1)[-1] in ('air','cave_air','void_air'): continue
            y=sy*16+ly
            if highest is None: highest=(y,name)
            if ground is None and ground_material(name): ground=(y,name)
          if ground is not None: break
        out.append({'x':x,'z':z,'top_y':highest[0] if highest else None,'top_block':highest[1] if highest else None,
                    'ground_y':ground[0] if ground else None,'ground_block':ground[1] if ground else None})
    return out

fixture={'schema':1,'provenance':{'kind':'independent_python_read_only_anvil_nbt','world_alias':'0','dimension':'minecraft:overworld',
 'notes':['Each row is one of all 256 local columns in a selected saved chunk. Ground is the highest saved block in the explicit terrain-material family list; top fields show foliage, snow or structure cover. No Minecraft launch, chunk generation or save writes.']},'sites':[]}
for site in SITES:
    cx,cz=site['chunk']; root,path=read_chunk(cx,cz)
    if root is None: continue
    cols=columns(root)
    if cols is None: continue
    fixture['sites'].append({'name':site['name'],'chunk':[cx,cz],'region':path.name,
      'region_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'columns':cols})
OUT.parent.mkdir(parents=True,exist_ok=True)
OUT.write_text(json.dumps(fixture,separators=(',',':'))+'\n')
print(f"captured {len(fixture['sites'])} chunks / {sum(len(s['columns']) for s in fixture['sites'])} columns -> {OUT}")
