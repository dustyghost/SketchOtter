import base64, gzip, hashlib, importlib.util, json, os, platform, sys, time
from pathlib import Path
from types import SimpleNamespace
spec=importlib.util.spec_from_file_location('capacity',Path('scripts/benchmark-capacity.py'))
c=importlib.util.module_from_spec(spec);spec.loader.exec_module(c)
exe, name, level=sys.argv[1:4];level=int(level)
out=Path(os.environ['RUNNER_TEMP'])/'capacity-results'/f'region-{name}-{level}'
out.mkdir(parents=True,exist_ok=False)
c.TEST='viewport::gpu::profile::capacity::'+{'picking':'picking::gpu_load_profile_world_picking','drawing':'drawing::gpu_load_profile_dense_materials','display':'regions::gpu_load_profile_changed_regions','release':'release::gpu_load_profile_release_fans','visual':'release::gpu_load_profile_release_visual','session':'session::gpu_load_profile_mixed_session','boundary':'session::gpu_load_profile_upper_boundary'}[name]
rss=int(os.environ.get("REGION_RSS_MIB","8000"))
reserve=int(os.environ.get("REGION_RESERVE_MIB","1200"))
options=SimpleNamespace(max_rss_mib=rss,min_available_mib=reserve,timeout=1800)
env=dict(os.environ, CLAY_REGION_LEVEL=str(level), CLAY_REGION_OUTPUT=str((out/'samples.json').resolve()))
if name=='session':
 import subprocess
 asset=Path('target/region-assets').resolve()
 subprocess.run([sys.executable,'scripts/prepare-capacity-asset.py',str(asset)],check=True)
 env['CLAY_REGION_ASSET']=str(asset)
before=c.proc_kib('/proc/meminfo','MemAvailable')
with c.workload_lock():
 if before is None or before<(rss+reserve)*1024: raise SystemExit('insufficient RAM preflight')
 result=c.bounded_run(exe,env,out/'case.log',options,test_name=c.TEST)
info=dict(status=result,preflight_kib=before,capacity_kib=c.proc_kib('/proc/meminfo','MemTotal'),cpu=next(s.split(':',1)[1].strip() for s in Path('/proc/cpuinfo').read_text().splitlines() if s.startswith('model name')),rss_mib=rss,reserve_mib=reserve,platform=platform.platform(),commit=c.command('git','rev-parse','HEAD'),rustc=c.command('rustc','--version'),binary_sha256=c.file_digest(Path(exe)),utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()))
(out/'environment.json').write_text(json.dumps(info,indent=2)+'\n')
print(info,flush=True)
if result['status']!='measured':
 print((out/'case.log').read_text()[-4000:])
 guarded=any(word in result.get('reason','') for word in ['RSS budget','system available','timeout'])
 if name!='boundary' or not guarded or not (out/'samples.json').is_file(): raise SystemExit(1)
pack={'env':info,'data':json.loads((out/'samples.json').read_text())}
if name=='visual':
 pack['pngs']={p.stem:base64.b64encode(p.read_bytes()).decode() for p in out.glob('*.png')}
blob=base64.b64encode(gzip.compress(json.dumps(pack,separators=(',',':')).encode())).decode()
for i in range(0,len(blob),16000): print('REGION_RECORD',name,level,i//16000,blob[i:i+16000])
