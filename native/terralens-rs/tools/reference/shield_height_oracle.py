#!/usr/bin/env python3
"""Compile/run the installed TFG dormant-shield height oracle without Minecraft."""
from __future__ import annotations
import argparse
import pathlib
import subprocess
import tempfile

DEFAULT_INSTANCE = pathlib.Path.home() / ".local/share/PrismLauncher/instances/TerraFirmaGreg-Modern/minecraft"
DEFAULT_JAVA = pathlib.Path(__file__).with_name("TerralensShieldHeightOracle.java")
DEFAULT_OUTPUT = pathlib.Path(__file__).resolve().parents[2] / "tests/fixtures/tfg-dormant-shield-java.json"
# Forge 1.20.1 Mth.map bytecode invokes inverseLerp(value,min,max), then lerp(t,outMin,outMax).
# The official bodies are (value-min)/(max-min) and outMin + t*(outMax-outMin).
MTH_STUB = '''package net.minecraft.util;\npublic final class Mth {\n public static double m_144914_(double v,double a,double b,double c,double d) {\n  double t=(v-a)/(b-a); return c+t*(d-c);\n }\n}\n'''
def main() -> None:
    ap=argparse.ArgumentParser()
    ap.add_argument('--instance',type=pathlib.Path,default=DEFAULT_INSTANCE)
    ap.add_argument('--java-source',type=pathlib.Path,default=DEFAULT_JAVA)
    ap.add_argument('--output',type=pathlib.Path,default=DEFAULT_OUTPUT)
    a=ap.parse_args()
    libs=a.instance.parents[2]/'libraries'
    tfg=a.instance/'mods/TerraFirmaGreg-Core-Modern-0.9.25.jar'
    mc=libs/'net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-srg.jar'
    fast=libs/'it/unimi/dsi/fastutil/8.5.12/fastutil-8.5.12.jar'
    for p in (tfg,mc,fast,a.java_source):
        if not p.is_file(): raise SystemExit(f'missing oracle input: {p}')
    cp=':'.join(map(str,(tfg,mc,fast)))
    with tempfile.TemporaryDirectory(prefix='tfg-shield-oracle-') as tmp:
        root=pathlib.Path(tmp); stub=root/'src/net/minecraft/util/Mth.java'; stub.parent.mkdir(parents=True)
        stub.write_text(MTH_STUB); classes=root/'classes'; classes.mkdir()
        subprocess.run(['javac','-d',str(classes),str(stub)],check=True)
        subprocess.run(['javac','-cp',f'{cp}:{classes}','-d',str(classes),str(a.java_source)],check=True)
        proc=subprocess.run(['java','-cp',f'{classes}:{cp}','TerralensShieldHeightOracle'],check=True,capture_output=True,text=True)
    a.output.write_text(proc.stdout); print(f'wrote {a.output}: {len(proc.stdout)} bytes')
if __name__=='__main__': main()
