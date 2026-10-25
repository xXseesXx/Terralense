#!/usr/bin/env python3
"""Capture original installed TFG 0.9.25 3D simplex/cellular classes as a fixture."""
import hashlib
import json
import pathlib
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
PROFILE = pathlib.Path.home()/'.local/share/PrismLauncher/instances/TerraFirmaGreg-Modern/minecraft'
CORE = PROFILE/'mods/TerraFirmaGreg-Core-Modern-0.9.25.jar'
LIBS = PROFILE.parents[2]/'libraries'
FAST = LIBS/'it/unimi/dsi/fastutil/8.5.12/fastutil-8.5.12.jar'
MC = LIBS/'net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-srg.jar'

with tempfile.TemporaryDirectory(prefix='terralens-noise-oracle-') as tmp:
    cp = ':'.join(map(str, (CORE, FAST, MC)))
    subprocess.run(['javac','-cp',cp,'-d',tmp,str(HERE/'SurfaceNoiseOracle.java')],check=True)
    result = subprocess.run(['java','-cp',f'{tmp}:{cp}','SurfaceNoiseOracle'],check=True,capture_output=True,text=True)
    data = json.loads(result.stdout)
data['provenance'] = {'source':'Installed original TFG/TFC classes, no Minecraft world',
                      'core_sha256':hashlib.sha256(CORE.read_bytes()).hexdigest(),
                      'classes':['OpenSimplex3D','Cellular3D'],
                      'snow_ice':'not sampled'}
out = ROOT/'tests/fixtures/generator-accuracy/surface-noise-java.json'
out.write_text(json.dumps(data,indent=2)+'\n')
print(out)
