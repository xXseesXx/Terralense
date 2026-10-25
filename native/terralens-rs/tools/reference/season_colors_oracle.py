#!/usr/bin/env python3
"""Independent Java transcription of pinned color math with upstream PNGs.

This is a numeric/source control, NOT a Forge/client runtime or Minecraft render test.
It never opens or modifies a Minecraft save.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import zipfile

JAVA = r'''
import java.awt.image.BufferedImage;
import java.io.*;
import javax.imageio.ImageIO;
import java.util.*;
public class SeasonColorOracle {
 static final Map<String,BufferedImage> PNG = new HashMap<>();
 static String dir;
 static int pixel(String name,int x,int y)throws Exception {
  BufferedImage image=PNG.get(name);
  if(image==null){image=ImageIO.read(new File(dir,name+".png"));PNG.put(name,image);}
  return image.getRGB(Math.max(0,Math.min(255,x)),Math.max(0,Math.min(255,y)))&0xffffff;
 }
 static int clamp(int n){return Math.max(0,Math.min(255,n));}
 static int climate(String image,float temp,float rain)throws Exception{
  return pixel(image,255-clamp((int)((temp+20f)*255f/50f)),255-clamp((int)(rain*255f/500f)));
 }
 static float adjusted(int y,float temp){return y>63?temp-Math.min(17.822f,Math.max(0f,(y-63)*.16225f)):temp;}
 static float tri(float a,float mid,float freq,float v){return mid+a*(Math.abs(4f*freq*v+1f-4f*(int)Math.floor(freq*v+.75f))-1f);}
 static int hash(long salt,int x,int y,int z){long h=salt^((long)x*501125321)^((long)y*1136930381)^z;h*=0x27d4eb2d;return(int)h;}
 static float monthly(float avg,int y,int z,float scale,float date){
  final float[] mods={-1f,-.866f,-.5f,0f,.5f,.866f,1f,.866f,.5f,0f,-.5f,-.866f};
  float m=date*12f;int i=((int)m)%12;float factor=mods[i]+(m-(int)m)*(mods[(i+1)%12]-mods[i]);
  float delta=factor*(scale==0?0:tri(-18f,0f,1f/(4f*scale),z-scale/2));
  if(y>63)return adjusted(y,avg)+delta;
  if(y>0)return avg+(y/63f)*delta;
  return 15f+((y-(-64f))/(0f-(-64f)))*(avg-15f);
 }
 static int generic(float avg,float rain,int x,int y,int z,float scale,float date,int autumn,boolean conifer)throws Exception{
  if(conifer)return climate("foliage",monthly(avg,y,z,scale,date),rain);
  float temp=adjusted(y,avg),t=temp>12f?12f:Math.max(temp,-20f);
  float cube=1.5f*(float)Math.pow(t+3f,3f)/4913f;
  float square=.5f*(float)Math.pow(t+3f,2f)/289f;
  float start=(cube+square+8.5f)/12f;
  float end=temp>12f?start:(cube-square+10.5f)/12f;
  float spring=1f-end;
  if(date>end)return climate("foliage_winter",temp,rain);
  else if(date>start){int noise=(hash(836494186029734123L,x,y,z)&127)-63;int progress=(int)Math.max(0f,Math.min(255f,255f*(date-start)/(end-start)+noise));return pixel("foliage_fall",progress,autumn);}
  else if(date>spring)return climate("foliage",monthly(avg,y,z,scale,date),rain);
  else return climate("foliage_winter",temp,rain);
 }
 static int noisyMonth(float date,int x,int y,int z){
  float months=date*12f;int month=(int)months;float d=(months-month)-.5f;
  float chance=16f*(d*d*d*d);int h=hash(836494186029734123L,x,y,z)&255;
  if(h<256f*chance)month=d>=.5?((month+1)%12):((month+11)%12);
  return month;
 }
 static int foliage(float avg,float rain,int x,int y,int z,float scale,float date,int autumn,boolean conifer,String species)throws Exception{
  if(species.equals("coconut")||species.equals("oil_palm")||species.equals("date")||species.equals("acai")){
   int row,minH,maxH=100;float minT,maxT;String active;
   switch(species){
    case "coconut":row=150;minH=35;minT=15;maxT=45;active="000011100111";break;
    case "oil_palm":row=200;minH=10;minT=19;maxT=62;active="100111100011";break;
    case "date":row=255;minH=40;minT=20;maxT=50;active="000000001111";break;
    default:row=255;minH=10;minT=24;maxT=70;active="111100000000";break;
   }
   int hydration=(int)(rain/5);float temp=avg;
   if(hydration>=minH&&hydration<=maxH&&avg>=minT&&avg<=maxT){if(active.charAt((int)(date*12f))=='0'){row+=50;temp+=50;}}
   else{row+=100;temp+=100;}
   int tx=255-clamp((int)((temp+20f)*255f/50f));tx+=(hash(836494186029734123L,x,y,z)&7)-3;
   return pixel("foliage",tx,clamp(row));
  }
  int month=noisyMonth(date,x,y,z);String blossom=null;
  if(species.equals("ipe")&&(month==2||month==3))blossom="foliage_yellow";
  if(species.equals("giant_rosewood")&&(month==2||month==3))blossom="foliage_jacaranda";
  if(species.equals("red_silk_cotton")&&(month==3||month==4))blossom="foliage_red";
  if(species.equals("flame_of_the_forest")&&(month==2||month==3))blossom="foliage_orange";
  if(blossom!=null){int h=hash(91273491823412341L,x,y,z)&65535;return pixel(blossom,h&255,h>>8);}
  return generic(avg,rain,x,y,z,scale,date,autumn,conifer);
 }
 public static void main(String[]args)throws Exception{
  dir=args[0];
  if(args.length>1&&args[1].equals("boundaries")){
   for(float avg:new float[]{-20f,-3f,0f,10f,12f,12.000001f,20f}){
    float t=avg>12f?12f:Math.max(avg,-20f);
    float cube=1.5f*(float)Math.pow(t+3f,3f)/4913f,square=.5f*(float)Math.pow(t+3f,2f)/289f;
    float start=(cube+square+8.5f)/12f,end=avg>12f?start:(cube-square+10.5f)/12f;
    for(float v:new float[]{start,end,1f-end})for(float d:new float[]{Math.nextDown(v),v,Math.nextUp(v)})System.out.println(avg+"\t"+d);
   }
   return;
  }
  BufferedReader reader=new BufferedReader(new InputStreamReader(System.in));String line;
  while((line=reader.readLine())!=null){
   String[]v=line.split("\\t");String kind=v[0],species=v[8];
   float avg=Float.parseFloat(v[1]),rain=Float.parseFloat(v[2]),scale=Float.parseFloat(v[6]);int x=Integer.parseInt(v[3]),y=Integer.parseInt(v[4]),z=Integer.parseInt(v[5]);
   boolean mean=v[7].equals("none"),conifer=Boolean.parseBoolean(v[10]);float date=mean?0:Float.parseFloat(v[7]);if(date<0||date>=1)date=(date%1f+1f)%1f;
   int rgb;
   if(mean)rgb=climate(kind.equals("grass")?"grass":kind.equals("water")?"water":"foliage",adjusted(kind.equals("water")?63:y,avg),rain);
   else if(kind.equals("grass"))rgb=climate("grass",monthly(avg,y,z,scale,date),rain);
   else if(kind.equals("water"))rgb=climate("water",monthly(avg,63,z,scale,date),rain);
   else rgb=foliage(avg,rain,x,y,z,scale,date,Integer.parseInt(v[9]),conifer,species);
   System.out.println(((rgb>>16)&255)+","+((rgb>>8)&255)+","+(rgb&255));
  }
 }
}
'''


def make_rows():
    rows=[]
    def add(kind,temp,rain,x,y,z,scale,season,species='',index=0,conifer=False):
        rows.append(dict(kind=kind,temperature=temp,rainfall=rain,x=x,y=y,z=z,
                         temperature_scale=scale,season=season,species=species,
                         autumn_index=index,conifer=conifer))
    # Climate anchors at equator, both hemispheres, underground openings and custom scales.
    climates=[(10.,250.,120,63,0,20000.),(10.,250.,-121,100,-10000,20000.),
              (10.,250.,120,63,10000,20000.),(10.,250.,-121,63,30000,20000.),
              (-15.,475.,-514,180,-24000,12000.),(15.,75.,1001,-32,18000,12000.),
              (10.,250.,120,0,0,20000.),(10.,250.,120,63,0,0.)]
    for kind,climate,date in itertools.product(['grass','water'],climates,[None,0.,.25,.5,.75,.999]):
        add(kind,*climate,date)
    for climate,date,(species,index,conifer) in itertools.product(climates[:5],
        [None,0.,.5,.75,.875,.99], [('oak',120,False),('pine',0,True),('pine_krummholz',0,False),('baobab',212,False),('cypress',0,True)]):
        add('foliage',*climate,date,species,index,conifer)
    # Warm threshold, cold autumn range, and signed-coordinate autumn hash controls.
    for temp in [-20.,-3.,0.,10.,12.,12.000001,20.]:
        for date in [.10,.70,.80,.90]:
            add('foliage',temp,250.,-2001,83,5001,20000.,date,'oak',120,False)
    for species,index in [('ipe',254),('giant_rosewood',190),('red_silk_cotton',150),('flame_of_the_forest',0)]:
        for date in [2.1/12,2.5/12,3.1/12,3.5/12,4.5/12,7.5/12]:
            add('foliage',10.,250.,-120,63,2345,20000.,date,species,index,False)
    for species in ['coconut','oil_palm','date','acai']:
        for date,temp,rain in [(0.,25.,250.),(.25,25.,250.),(.5,25.,250.),(.75,25.,250.),(.5,14.,250.),(.75,25.,25.)]:
            add('foliage',temp,rain,-120,100,2345,20000.,date,species,0,False)
    return rows


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=Path,required=True)
    parser.add_argument('--afc-jar',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    rows=make_rows(); hashes={}
    with tempfile.TemporaryDirectory(prefix='terralens-season-oracle-') as tmp:
        tmp=Path(tmp)
        for name in ['grass','foliage','foliage_fall','foliage_winter','water']:
            asset=args.source/'src/main/resources/assets/tfc/textures/colormap'/f'{name}.png'
            shutil.copyfile(asset,tmp/f'{name}.png');hashes[f'tfc/{name}.png']=hashlib.sha256(asset.read_bytes()).hexdigest()
        with zipfile.ZipFile(args.afc_jar) as jar:
            for name in ['foliage_jacaranda','foliage_yellow','foliage_red','foliage_orange']:
                asset=jar.read(f'assets/tfc/textures/colormap/{name}.png')
                (tmp/f'{name}.png').write_bytes(asset);hashes[f'afc/{name}.png']=hashlib.sha256(asset).hexdigest()
        (tmp/'SeasonColorOracle.java').write_text(JAVA)
        subprocess.run(['javac',str(tmp/'SeasonColorOracle.java')],check=True,capture_output=True)
        bounds=subprocess.run(['java','-Djava.awt.headless=true','-cp',str(tmp),'SeasonColorOracle',str(tmp),'boundaries'],
                              text=True,capture_output=True,check=True).stdout.splitlines()
        for line in bounds:
            temp,date=map(float,line.split())
            rows.append(dict(kind='foliage',temperature=temp,rainfall=250.,x=-121,y=63,z=5001,
                             temperature_scale=20000.,season=date,species='oak',autumn_index=120,conifer=False))
        payload='\n'.join('\t'.join(str(r[k]) if r[k]is not None else'none' for k in
            ['kind','temperature','rainfall','x','y','z','temperature_scale','season','species','autumn_index','conifer'])for r in rows)+'\n'
        # Java Boolean.parseBoolean accepts Python's True/False text case-insensitively.
        proc=subprocess.run(['java','-Djava.awt.headless=true','-cp',str(tmp),'SeasonColorOracle',str(tmp)],
                            input=payload,text=True,capture_output=True,check=True)
        colors=proc.stdout.splitlines();assert len(colors)==len(rows)
        for row,color in zip(rows,colors):row['expected_rgb']=list(map(int,color.split(',')))
    result={'schema':1,'provenance':{
        'kind':'standalone_java_transcription_of_pinned_source_math_plus_unmodified_upstream_png',
        'scope':'Numeric source control; not a Forge client or native Minecraft rendering test.',
        'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=args.source,text=True).strip(),
        'afc_jar_sha256':hashlib.sha256(args.afc_jar.read_bytes()).hexdigest(),
        'oracle_java_sha256':hashlib.sha256(JAVA.encode()).hexdigest(),
        'texture_sha256':hashes,
        'source_methods':['OverworldClimateModelMixin.calculateMonthlyTemperature','OverworldClimateModel.adjustTemperatureByElevation',
         'TFCColors.getSeasonalFoliageColor','TFCColors.getAutumnColor','Helpers.hash','ClientEventHandler.registerColorHandlerBlocks',
         'AFC ModEventClientBusEvents.registerColorHandlerBlocks','AFCColors.getAdjustedNoisyMonth','PalmColorProvider.getPalmFoliageColor'],
        'intentional_divergence':'None date preserves historical TerraLens annual mean rather than a live calendar. Daily/time-of-day temperature is set to zero.',
    },'samples':rows}
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(f'{len(rows)} Java RGB controls -> {args.output}')

if __name__=='__main__':main()
