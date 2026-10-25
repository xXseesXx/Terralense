#!/usr/bin/env python3
"""Run unmodified TFG 0.9.25 centered-feature samplers with metadata-only stubs.

Requires the installed Prism profile and Forge mapped caches at this machine's
standard paths. Java's original noise, cellular, seed, Mth, and sampler classes
are loaded from the mapped Core/Forge jars; only biome registry metadata is
stubbed so the sampler can run without booting Minecraft.
"""
import hashlib, json, os, pathlib, subprocess, tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
CORE = pathlib.Path.home()/'.gradle/caches/forge_gradle/deobf_dependencies/local/TerraFirmaGreg-Core-Modern/0.9.25_mapped_official_1.20.1/TerraFirmaGreg-Core-Modern-0.9.25_mapped_official_1.20.1.jar'
MC = pathlib.Path.home()/'.gradle/caches/forge_gradle/minecraft_user_repo/net/minecraftforge/forge/1.20.1-47.4.13_mapped_official_1.20.1/forge-1.20.1-47.4.13_mapped_official_1.20.1.jar'
TFC_MODS = pathlib.Path.home()/'.local/share/PrismLauncher/instances/TerraFirmaGreg-Modern/minecraft/mods'
OUT = ROOT/'tests/fixtures/tfg-centered-volcanic-java.json'

PROFILES = [
 # id, feature, enabled flags, rarity, base, scale, ice, active identity
 ('canyons','cinder',1,0,0,6,14,30,False,False),
 ('volcanic_mountains','cinder',1,0,0,4,25,50,False,False),
 ('volcanic_oceanic_mountains','cinder',1,0,0,2,-12,50,False,False),
 ('active_shield_volcano','cinder',1,0,0,4,15,25,False,True),
 ('dormant_shield_volcano','tuff',0,1,0,2,0,36,False,False),
 ('extinct_shield_volcano','tuff',0,1,0,2,0,26,False,False),
 ('ancient_shield_volcano','tuff',0,1,0,3,-16,30,False,False),
 ('sunken_shield_volcano','tuff',0,1,0,2,-8,24,False,False),
 ('old_shield_volcano_shore','tuff',0,1,0,3,-8,26,False,False),
 ('tuyas','tuya',0,0,1,2,0,35,False,False),
 ('ice_sheet_tuyas','tuya',0,0,1,3,0,35,True,False),
 ('ice_sheet_tuyas_edge','tuya',0,0,1,3,0,35,True,False),
]

JAVA = r'''import java.util.*; import net.dries007.tfc.world.biome.*;
import su.terrafirmagreg.core.world.new_ow_wg.*;
import su.terrafirmagreg.core.world.new_ow_wg.biome.*;
import su.terrafirmagreg.core.world.new_ow_wg.noise.*;
import su.terrafirmagreg.core.world.new_ow_wg.shores.ShoreNoiseSampler;
public class Runner {
 static class Profile extends BiomeExtension {
  boolean c,t,u,ice; int rarity,base,scale;
  Profile(boolean c,boolean t,boolean u,int r,int b,int s,boolean i){this.c=c;this.t=t;this.u=u;rarity=r;base=b;scale=s;ice=i;}
  public boolean tfg$hasCinderCones(){return c;} public boolean tfg$hasTuffRings(){return t;} public boolean tfg$hasTuyas(){return u;}
  public int tfg$getCenteredFeatureRarity(){return rarity;} public int tfg$getCenteredFeatureBaseHeight(){return base;} public int tfg$getCenteredFeatureScaleHeight(){return scale;} public boolean tfg$getCenteredFeatureIce(){return ice;}
 }
 static class Source implements BiomeSourceExtension { final BiomeExtension b; Source(BiomeExtension b){this.b=b;} public BiomeExtension getBiomeExtension(int x,int z){return b;} }
 public static void main(String[] args) {
  String id=args[0], family=args[1]; long seed=Long.parseLong(args[2]); int rarity=Integer.parseInt(args[3]),base=Integer.parseInt(args[4]),scale=Integer.parseInt(args[5]); boolean ice=Boolean.parseBoolean(args[6]),active=Boolean.parseBoolean(args[7]);
  Profile p=new Profile(family.equals("cinder"),family.equals("tuff"),family.equals("tuya"),rarity,base,scale,ice);
  if(active) TFGBiomes.ACTIVE_SHIELD_VOLCANO=p;
  Source source=new Source(p);
  // Only tuffRing consumes Seed.next(); cinder and tuya use the unadvanced base seed.
  Seed s=Seed.of(seed);
  CenteredFeatureNoiseSampler sampler=family.equals("cinder")?CenteredFeatureNoise.cinder(s):family.equals("tuff")?CenteredFeatureNoise.tuffRing(s):CenteredFeatureNoise.tuya(s);
  int got=0, rejected=0; HashSet<String> used=new HashSet<>();
  // deterministic coordinate stream spans negative and positive world positions
  for(int k=0;k<250000 && got<18;k++) {
   int x=(int)(((long)k*7919L)%400001L)-200000;
   int z=(int)(((long)k*104729L+seed)%400001L)-200000;
   double pre= -12.25 + (k%9)*17.375;
   double out=sampler.setColumnAndSampleHeight(pre,x,z,source);
   boolean present=out!=Integer.MIN_VALUE;
   if(!present && rejected<4){emit(id,family,seed,x,z,pre,out,false);rejected++;}
   if(present && used.add(Math.floorDiv(x,256)+":"+Math.floorDiv(z,256))){emit(id,family,seed,x,z,pre,out,true);got++;}
  }
 }
 static void emit(String id,String family,long seed,int x,int z,double pre,double out,boolean present){System.out.printf(Locale.ROOT,"{\"profile\":\"%s\",\"family\":\"%s\",\"seed\":%d,\"x\":%d,\"z\":%d,\"preHeight\":%.17g,\"rawHeight\":%.17g,\"present\":%s,\"appliedHeight\":%.17g}%n",id,family,seed,x,z,pre,out,present,present?out:pre);}
}
class ShoreRunner {
 static net.dries007.tfc.world.biome.BiomeExtension biome(int base){return new net.dries007.tfc.world.biome.BiomeExtension(){public int tfg$getShoreBaseHeight(){return base;}};}
 static String jsonNum(double v){return Double.isFinite(v)?Double.toString(v):"\""+Double.toString(v)+"\"";}
 public static void main(String[] args){
  String[] names={"sandyBeach","setbackCliffs","dunes","embayments","rockyShores","classic","seaStacks","upperTerrace","lowerTerrace"};
  long[] seeds={0,1,-1,1234567890123456789L,-987654321098765432L};
  for(String name:names) for(long seed:seeds){
   Seed s=Seed.of(seed); ShoreNoiseSampler sampler=switch(name){
    case "sandyBeach"->ShoreNoise.sandyBeach(s); case "setbackCliffs"->ShoreNoise.setbackCliffs(s); case "dunes"->ShoreNoise.dunes(s); case "embayments"->ShoreNoise.embayments(s); case "rockyShores"->ShoreNoise.rockyShores(s); case "classic"->ShoreNoise.classic(s); case "seaStacks"->ShoreNoise.seaStacks(s); case "upperTerrace"->ShoreNoise.upperTerrace(s); default->ShoreNoise.lowerTerrace(s);};
   net.dries007.tfc.world.noise.Noise2D tide=TFGBiomeNoise.shoreTideLevelNoise(s);
   int[] bases=name.equals("classic")?new int[]{-4,0,-8,-12,-16,59,63,55,51,47}:new int[]{0};
   for(int baseIndex=0;baseIndex<bases.length;baseIndex++){ int base=bases[baseIndex]; String profile=name+(name.equals("classic")?(baseIndex<5?"_raw_offset_":"_getter_")+base:""); for(int k=0;k<24;k++){
    int x=(int)(((long)k*7919L+seed)%400001L)-200000, z=(int)(((long)k*104729L-seed)%400001L)-200000;
    double ocean=(k%6)*0.1, land=Math.max(0,0.75-ocean), shore=1.0-ocean-land;
    double input=45.0+(k%8)*8.25, shoreHeight=50.0+(k%5)*5.5, normalHeight=58.0+(k%7)*4.25;
    double column=sampler.setColumnAndSampleHeight(input,x,z,ocean,land,shore,shore,biome(base),shoreHeight,normalHeight), tideValue=tide.noise(x,z);
    double tideStage=ocean>=0.25?net.minecraft.util.Mth.clampedMap(land,0.32,0.36,Math.min(column,tideValue-4),column):column;
    System.out.printf(Locale.ROOT,"{\"family\":\"shore\",\"case\":\"grid\",\"profile\":\"%s\",\"seed\":%d,\"x\":%d,\"z\":%d,\"biome\":{\"shoreBaseHeight\":%d,\"shoreBaseInputKind\":\"%s\"},\"oceanWeight\":%.17g,\"landWeight\":%.17g,\"shoreWeight\":%.17g,\"thisWeight\":%.17g,\"heightIn\":%.17g,\"shoreHeight\":%.17g,\"normalHeight\":%.17g,\"columnHeight\":%s,\"columnNonFinite\":%s,\"tideLevel\":%.17g,\"tideStageHeight\":%.17g}%n",profile,seed,x,z,base,name.equals("classic")&&baseIndex<5?"synthetic_direct_getter_stress":"absolute_runtime_getter",ocean,land,shore,shore,input,shoreHeight,normalHeight,jsonNum(column),!Double.isFinite(column),tideValue,tideStage);
   }
   double[][] corners={{0,0,1},{0,1,0},{1,0,0},{0,1e-12,1-1e-12},{1-1e-12,0,1e-12},{0,1-1e-12,1e-12}};
   for(int j=0;j<corners.length;j++){
    int x=1729+j*997, z=-2471-j*613; double ocean=corners[j][0], land=corners[j][1], shore=corners[j][2];
    double thisWeight=shore, input=name.equals("classic")&&j==0?90.0:72.0, shoreHeight=75.0, normalHeight=name.equals("classic")&&j==0?0.0:64.0;
    double column=sampler.setColumnAndSampleHeight(input,x,z,ocean,land,shore,thisWeight,biome(base),shoreHeight,normalHeight), tideValue=tide.noise(x,z);
    double tideStage=ocean>=0.25?net.minecraft.util.Mth.clampedMap(land,0.32,0.36,Math.min(column,tideValue-4),column):column;
    System.out.printf(Locale.ROOT,"{\"family\":\"shore\",\"case\":\"weight_corner_%d\",\"profile\":\"%s\",\"seed\":%d,\"x\":%d,\"z\":%d,\"biome\":{\"shoreBaseHeight\":%d,\"shoreBaseInputKind\":\"%s\"},\"oceanWeight\":%.17g,\"landWeight\":%.17g,\"shoreWeight\":%.17g,\"thisWeight\":%.17g,\"heightIn\":%.17g,\"shoreHeight\":%.17g,\"normalHeight\":%.17g,\"columnHeight\":%s,\"columnNonFinite\":%s,\"tideLevel\":%.17g,\"tideStageHeight\":%s}%n",j,profile,seed,x,z,base,name.equals("classic")&&baseIndex<5?"synthetic_direct_getter_stress":"absolute_runtime_getter",ocean,land,shore,thisWeight,input,shoreHeight,normalHeight,jsonNum(column),!Double.isFinite(column),tideValue,jsonNum(tideStage));
   }
   }
  }
 }
}'''

def main():
    assert CORE.is_file() and MC.is_file(), 'Mapped Forge/Core jars are missing'
    mods=list(TFC_MODS.glob('TerraFirmaGreg-Core-Modern-0.9.25.jar'))
    libs=list(pathlib.Path.home().glob('.local/share/PrismLauncher/libraries/**/*.jar'))
    # A minimal library selection avoids Prism's parallel game versions causing
    # binary-incompatible Log4j/SLF4J classes at Minecraft Mth initialization.
    libs=[p for p in libs if ('org/apache/logging/log4j/' not in p.as_posix() or '/2.19.0/' in p.as_posix())
          and ('org/slf4j/' not in p.as_posix() or '/2.0.' in p.as_posix())]
    jars=[CORE,MC]+libs
    cp=os.pathsep.join(map(str,dict.fromkeys(jars)))
    with tempfile.TemporaryDirectory(prefix='tfg-java-oracle-') as td:
        d=pathlib.Path(td); src=d/'src'; classes=d/'classes'
        def put(rel,txt):
            p=src/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(txt)
        put('net/dries007/tfc/world/biome/BiomeExtension.java','''package net.dries007.tfc.world.biome; public class BiomeExtension implements su.terrafirmagreg.core.world.new_ow_wg.biome.IBiomeExtension { public boolean tfg$hasCinderCones(){return false;} public boolean tfg$hasTuffRings(){return false;} public boolean tfg$hasTuyas(){return false;} public int tfg$getCenteredFeatureRarity(){return 0;} public int tfg$getCenteredFeatureBaseHeight(){return 0;} public int tfg$getCenteredFeatureScaleHeight(){return 0;} public boolean tfg$getCenteredFeatureIce(){return false;} public int tfg$getShoreBaseHeight(){return 0;} }''')
        put('net/dries007/tfc/world/biome/BiomeSourceExtension.java','''package net.dries007.tfc.world.biome; public interface BiomeSourceExtension { BiomeExtension getBiomeExtension(int x,int z); }''')
        put('su/terrafirmagreg/core/world/new_ow_wg/biome/IBiomeExtension.java','''package su.terrafirmagreg.core.world.new_ow_wg.biome; public interface IBiomeExtension { boolean tfg$hasCinderCones(); boolean tfg$hasTuffRings(); boolean tfg$hasTuyas(); int tfg$getCenteredFeatureRarity(); int tfg$getCenteredFeatureBaseHeight(); int tfg$getCenteredFeatureScaleHeight(); boolean tfg$getCenteredFeatureIce(); int tfg$getShoreBaseHeight(); }''')
        put('su/terrafirmagreg/core/world/new_ow_wg/biome/TFGBiomes.java','''package su.terrafirmagreg.core.world.new_ow_wg.biome; public final class TFGBiomes { public static net.dries007.tfc.world.biome.BiomeExtension ACTIVE_SHIELD_VOLCANO; }''')
        put('Runner.java',JAVA); classes.mkdir()
        subprocess.run(['javac','-cp',cp,'-d',str(classes),*map(str,src.rglob('*.java'))],check=True)
        rows=[]
        proc=subprocess.run(['java','-cp',str(classes)+os.pathsep+cp,'ShoreRunner'],text=True,capture_output=True)
        if proc.returncode:
            raise RuntimeError(proc.stderr[-5000:])
        rows.extend(json.loads(line) for line in proc.stdout.splitlines())
        seeds=[0,1,-1,1234567890123456789,-987654321098765432]
        for prof in PROFILES:
            id,fam,c,t,u,rarity,base,scale,ice,active=prof
            for seed in seeds:
                args=['java','-cp',str(classes)+os.pathsep+cp,'Runner',id,fam,str(seed),str(rarity),str(base),str(scale),str(ice).lower(),str(active).lower()]
                proc=subprocess.run(args,text=True,capture_output=True)
                if proc.returncode:
                    raise RuntimeError(proc.stderr[-5000:])
                for line in proc.stdout.splitlines():
                    row=json.loads(line)
                    row['biome']={'hasCinderCones':bool(c),'hasTuffRings':bool(t),'hasTuyas':bool(u),
                                  'rarity':rarity,'baseHeight':base,'scaleHeight':scale,'icy':ice,
                                  'isActiveShieldVolcano':active}
                    rows.append(row)
    jar_hash=hashlib.sha256(CORE.read_bytes()).hexdigest()
    fixture={'format':'tfg-centered-volcanic-java-v1','provenance':{'source':'TerraFirmaGreg-Core-Modern 0.9.25','sourceCommit':'f4929e738e83dbc8d46680276faa67520dffa172','mappedCoreJarSha256':jar_hash,'samplers':'Unmodified CenteredFeatureNoise and ShoreNoise classes from mapped Core jar; metadata-only biome/source stubs; original TFGCellular2D, TFC OpenSimplex2D, Seed/Xoroshiro, and Minecraft Mth loaded from jars.','heightMeaning':'Centered rawHeight is original sampler output (Integer.MIN_VALUE means absent); appliedHeight matches TFGChunkHeightFiller sentinel handling for the constant single-feature profile: rawHeight when present, otherwise preHeight. Shore columnHeight and tideLevel come from original Java samplers; tideStageHeight applies TFGChunkHeightFiller ocean-edge clamp using Minecraft Mth. Shore `shoreBaseHeight` is the absolute tfg$getShoreBaseHeight getter passed to the original sampler. `classic_raw_offset_*` preserve synthetic direct-getter stress cases; `classic_getter_*` use runtime getter values produced by SEA_LEVEL_Y + raw biome offsets. Non-finite shore returns are JSON strings. Classic pure-shore corners return original heightIn after the internal 0/0 expression becomes NaN.'},'samples':rows}
    OUT.parent.mkdir(parents=True,exist_ok=True); OUT.write_text(json.dumps(fixture,indent=2)+'\n')
    print(f'wrote {len(rows)} oracle samples to {OUT}')
if __name__=='__main__': main()
