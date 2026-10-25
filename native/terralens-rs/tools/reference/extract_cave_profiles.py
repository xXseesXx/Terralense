#!/usr/bin/env python3
"""Extract cave settings from installed TFG and the pinned matching source tree.

Usage: extract_cave_profiles.py /path/to/tfg-core-0.9.25
The source checkout must be f4929e738e83dbc8d46680276faa67520dffa172.
No world files are read or written.
"""
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE = pathlib.Path(sys.argv[1]).resolve()
PIN = 'f4929e738e83dbc8d46680276faa67520dffa172'
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip() == PIN
JAR = pathlib.Path.home()/'.local/share/PrismLauncher/instances/TerraFirmaGreg-Modern/minecraft/mods/TerraFirmaGreg-Core-Modern-0.9.25.jar'
OUT = ROOT/'src/caves/assets'
java = SOURCE/'src/main/java'
files = {}

def source(path):
    p = java/path
    files[str(path)] = hashlib.sha256(p.read_bytes()).hexdigest()
    return p.read_text()

def write(name, value):
    (OUT/name).write_text(json.dumps(value,indent=2)+'\n')

with zipfile.ZipFile(JAR) as jar:
    def read(path): return json.loads(jar.read(path))
    ids = ['noise_caves','noodle_caves','noodle_thickness','noodle_ridge_a','noodle_ridge_b']
    write('density.json',{k:read(f'data/tfc/worldgen/density_function/{k}.json') for k in ids})
    carvers = {path.rsplit('/',1)[1][:-5]:read(path) for path in jar.namelist()
               if path.startswith('data/tfc/worldgen/configured_carver/') and path.endswith('.json')}
    biomes = {path.rsplit('/',1)[1][:-5]:read(path)['carvers']['air'] for path in jar.namelist()
              if path.startswith('data/tfg/worldgen/biome/earth/') and path.endswith('.json')}
    write('carvers.json',{'carvers':carvers,'biomes':biomes})

s = source(pathlib.Path('su/terrafirmagreg/core/world/new_ow_wg/biome/TFGBiomes.java'))
profiles = {}
for match in re.finditer(r'public static final BiomeExtension \w+ = register\("([^"]+)"(.*?);',s,re.S):
    name, body = match.groups()
    offset = re.search(r'\.aquiferHeightOffset\((-?\d+)\)',body)
    lakes = '.carving(BiomeNoise::undergroundLakes)' in body
    profiles[name] = {'aquifer_offset':float(offset[1]) if offset else None,
                      'aquifer_fixed':47.0 if lakes else None,'underground_lakes':lakes}
assert len(profiles) == 109, len(profiles)
write('biomes.json',profiles)
noise = source(pathlib.Path('net/dries007/tfc/world/noise/FastNoiseLite.java'))
for field, out, size in [('RandVecs3D','cellular_vectors.json',1024),('Gradients3D','gradients3d.json',256)]:
    body = re.search(r'\b'+field+r'\s*=\s*\{(.*?)\};',noise,re.S)[1]
    values = [float(n.strip().removesuffix('f')) for n in body.split(',') if n.strip()]
    assert len(values) == size, (field,len(values))
    write(out,values)
write('provenance.json',{'source_commit':PIN,'jar_sha256':hashlib.sha256(JAR.read_bytes()).hexdigest(),
                         'source_sha256':files,'extractor':'tools/reference/extract_cave_profiles.py'})
