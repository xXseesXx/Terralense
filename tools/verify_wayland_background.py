#!/usr/bin/env python3
# Development-only Wayland protocol verification; never used by the Rust application.
# Usage: python3 tools/verify_wayland_background.py app|wait|nowait BINARY REPORT.json
# This opens only the supplied binary, switches focus in three cycles, injects protocol
# pings through an FD-preserving local proxy, closes its own window and restores focus.
# wait/nowait expect the Rust wayland_background_probe example. app expects TerraLens.
# A compositor-paused redraw counter is not used as a responsiveness criterion.
import socket,os,threading,struct,array,time,json,subprocess,pathlib,select
runtime=os.environ['XDG_RUNTIME_DIR']; display=os.environ['WAYLAND_DISPLAY']
real=display if display.startswith('/') else runtime+'/'+display
proxy=runtime+'/terralens-ping-test-'+str(os.getpid())
listener=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);listener.bind(proxy);listener.listen(8);listener.settimeout(.2)
stop=threading.Event(); states=[]; logs=[]; lock=threading.Lock(); started=time.monotonic()
def record(**row):
 row['t']=round(time.monotonic()-started,6)
 with lock:logs.append(row)
def loop(sock,dest,state,up):
 buf=bytearray();fds=[]
 try:
  while not stop.is_set():
   data,anc,flags,addr=sock.recvmsg(65536,socket.CMSG_SPACE(256*4))
   if not data:break
   buf.extend(data)
   for level,kind,payload in anc:
    if level==socket.SOL_SOCKET and kind==socket.SCM_RIGHTS:
     a=array.array('i');a.frombytes(payload[:len(payload)//4*4]);fds.extend(a)
   ready=bytearray()
   while len(buf)>=8:
    obj,head=struct.unpack_from('<II',buf); size=head>>16; opcode=head&65535
    if size<8 or size%4:raise RuntimeError('invalid Wayland framing')
    if len(buf)<size:break
    msg=bytes(buf[:size]);del buf[:size]
    if up and opcode==0 and size>=20:
     length=struct.unpack_from('<I',msg,12)[0]
     if length<256 and 16+((length+3)//4)*4+8<=size:
      iface=msg[16:16+length].rstrip(b'\0')
      if iface==b'xdg_wm_base':
       state['wm']=struct.unpack_from('<I',msg,size-4)[0];record(event='bind',wm=state['wm'])
    if up and obj==state.get('wm') and opcode==3 and size==12:
     serial=struct.unpack_from('<I',msg,8)[0]
     if serial in state['pending']:
      elapsed=(time.monotonic()-state['pending'].pop(serial))*1000
      record(event='pong',serial=serial,latency_ms=round(elapsed,4));continue
    ready.extend(msg)
   if ready:
    with state['up_lock' if up else 'down_lock']:
     ancillary=[(socket.SOL_SOCKET,socket.SCM_RIGHTS,array.array('i',fds))] if fds else []
     n=dest.sendmsg([ready],ancillary)
     if n<len(ready):dest.sendall(ready[n:])
    for fd in fds:os.close(fd)
    fds=[]
 except (OSError,RuntimeError) as e:record(event='relay_exit',direction='up' if up else 'down',reason=str(e))
 finally:
  for fd in fds:
   try:os.close(fd)
   except OSError:pass
  try:dest.shutdown(socket.SHUT_WR)
  except OSError:pass

def acceptor():
 while not stop.is_set():
  try:c,_=listener.accept()
  except socket.timeout:continue
  except OSError:break
  s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);s.connect(real)
  state={'client':c,'server':s,'pending':{},'up_lock':threading.Lock(),'down_lock':threading.Lock()};states.append(state)
  threading.Thread(target=loop,args=(c,s,state,True),daemon=True).start();threading.Thread(target=loop,args=(s,c,state,False),daemon=True).start()
threading.Thread(target=acceptor,daemon=True).start()
# Binary and flags supplied through argv; report path remains outside application storage.
import sys
if len(sys.argv)!=4 or sys.argv[1] not in ('app','wait','nowait'):
 raise SystemExit('Usage: verify_wayland_background.py app|wait|nowait BINARY REPORT.json')
mode=sys.argv[1];binary=sys.argv[2];report=sys.argv[3]
env=os.environ.copy();env['WAYLAND_DISPLAY']=proxy;env.pop('WAYLAND_DEBUG',None)
log=open('/tmp/terralens-wayland-controlled-'+mode+'.log','w')
initial=json.loads(subprocess.check_output(['hyprctl','activewindow','-j'])).get('address')
p=subprocess.Popen([binary]+(['--vsync'] if mode=='wait' else []),env=env,stdout=log,stderr=log,start_new_session=True)
win=None
for _ in range(100):
 for c in json.loads(subprocess.check_output(['hyprctl','clients','-j'])):
  if c['pid'] in (os.getpid(),p.pid) and c['title'].startswith('TerraLens' if mode=='app' else 'TerraLens Wayland Liveness Probe'):
   win=c;break
 if win and any('wm'in s for s in states):break
 time.sleep(.1)
record(event='start',pid=p.pid,mode=mode,window=win)
# Relayed clients are attributed to the proxy process; identify by the probe-only window title.
def focus(address):
 if address:subprocess.run(['hyprctl','dispatch','hl.dsp.focus({ window = "address:'+address+'" })'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
cover_process=None;cover_window=None
cover_binary=os.environ.get('TERRALENS_TEST_COVER')
if cover_binary:
 focus(win['address'])
 cover_process=subprocess.Popen([cover_binary,'--cover'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
 for _ in range(50):
  cover_window=next((c for c in json.loads(subprocess.check_output(['hyprctl','clients','-j'])) if c['pid']==cover_process.pid),None)
  if cover_window:break
  time.sleep(.1)
 record(event='cover',window=cover_window)
serial=0x6f100000
try:
 for phase,duration in [('foreground',3),('background',12),('foreground',2),('background',12),('foreground',2),('background',12)]:
  focus(win['address'] if phase=='foreground' else (cover_window['address'] if cover_window else initial))
  active=json.loads(subprocess.check_output(['hyprctl','activewindow','-j']))
  observed=next((c for c in json.loads(subprocess.check_output(['hyprctl','clients','-j'])) if c['address']==win['address']),{})
  record(event='phase',phase=phase,active_title=active.get('title'),probe_visible=observed.get('visible'),probe_workspace=observed.get('workspace'))
  deadline=time.monotonic()+duration
  while time.monotonic()<deadline:
   for state in states:
    if 'wm' not in state:continue
    serial+=1;state['pending'][serial]=time.monotonic()
    msg=struct.pack('<III',state['wm'],(12<<16)|0,serial)
    with state['down_lock']:state['client'].sendall(msg)
    record(event='ping',phase=phase,serial=serial)
   time.sleep(.5)
finally:
 record(event='finish',pending=sum(len(s['pending']) for s in states))
 # Close only our diagnostics, then restore the previously active client.
 if cover_window:subprocess.run(['hyprctl','dispatch','hl.dsp.window.close({ window = "address:'+cover_window['address']+'" })'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 if cover_process:
  try:cover_process.wait(timeout=2)
  except subprocess.TimeoutExpired:cover_process.terminate();cover_process.wait(timeout=2)
 if win:subprocess.run(['hyprctl','dispatch','hl.dsp.window.close({ window = "address:'+win['address']+'" })'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 try:p.wait(timeout=2)
 except subprocess.TimeoutExpired:p.terminate();p.wait(timeout=2)
 focus(initial);stop.set();listener.close()
 for state in states:
  for k in ('client','server'):
   try:state[k].close()
   except OSError:pass
 pathlib.Path(proxy).unlink(missing_ok=True)
 pathlib.Path(report).write_text(json.dumps({'mode':mode,'events':logs},indent=2))
 print(json.dumps({'mode':mode,'ping':sum(e['event']=='ping'for e in logs),'pong':sum(e['event']=='pong'for e in logs),'max_ms':max((e.get('latency_ms',0)for e in logs),default=0),'report':report}))
