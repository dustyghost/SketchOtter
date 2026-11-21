import base64,gzip,json,os,platform,re,subprocess,sys,time
from pathlib import Path
binary,level=sys.argv[1:];level=int(level)
out=Path(f'target/metal-{level}');out.mkdir(exist_ok=True)
def avail():
    stats=subprocess.check_output(['vm_stat'],text=True)
    page=int(re.search(r'page size of (\d+) bytes',stats)[1])
    return page*sum(int(re.search(r'^Pages '+key+r':\s+(\d+)',stats,re.M)[1]) for key in ['free','inactive','speculative'])
reserve=512<<20;budget=(1800 if level==3 else 3600)<<20
info={'platform':platform.platform(),'architecture':platform.machine(),'mem_bytes':int(subprocess.check_output(['sysctl','-n','hw.memsize'])),'initial_available':avail(),'budget':budget,'reserve':reserve}
print(info,flush=True)
if info['initial_available']<budget+reserve:
 print('METAL_GUARD_SKIP',level,flush=True);sys.exit(0)
env=dict(os.environ,CLAY_REGION_LEVEL=str(level),CLAY_REGION_OUTPUT=str((out/'samples.json').resolve()))
test='viewport::gpu::profile::capacity::drawing::gpu_load_profile_dense_materials'
proc=subprocess.Popen([binary,test,'--exact','--ignored','--nocapture','--test-threads=1'],env=env)
peak=0;low=info['initial_available'];deadline=time.monotonic()+1200
while proc.poll() is None:
 try: rss=int(subprocess.check_output(['ps','-o','rss=','-p',str(proc.pid)]))*1024
 except (ValueError,subprocess.CalledProcessError): continue
 headroom=avail();low=min(low,headroom);peak=max(peak,rss)
 if headroom<reserve or rss>budget or time.monotonic()>deadline:
  proc.terminate();proc.wait(timeout=20);print('METAL_GUARD_STOP',level,rss,headroom);sys.exit(0)
 time.sleep(.25)
if proc.returncode!=0:sys.exit(proc.returncode)
info.update(peak_rss_bytes=peak,minimum_available_bytes=low)
record={'env':info,'data':json.loads((out/'samples.json').read_text())}
compressed=base64.b64encode(gzip.compress(json.dumps(record,separators=(',',':')).encode())).decode()
for i in range(0,len(compressed),16000): print('REGION_RECORD','metal'+platform.machine(),level,i//16000,compressed[i:i+16000])
