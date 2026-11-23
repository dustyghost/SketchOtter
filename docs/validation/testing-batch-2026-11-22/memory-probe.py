import json, os, pathlib, signal, subprocess, sys, time
out = pathlib.Path(sys.argv[1])
cmd = sys.argv[2:]
limit = int(os.environ.get('CLAY_PROBE_LIMIT_MIB', '1200')) * 1024
start = time.monotonic()
peak = 0
reason = None
stall_since = None
with out.with_suffix('.log').open('w') as log:
    child = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    while child.poll() is None:
        rss = 0
        for entry in pathlib.Path('/proc').iterdir():
            if not entry.name.isdigit(): continue
            try:
                stat = (entry / 'stat').read_text().split(') ', 1)[1].split()
                if int(stat[3]) == child.pid:
                    rss += int(stat[21]) * os.sysconf('SC_PAGE_SIZE') // 1024
            except (FileNotFoundError, ProcessLookupError, PermissionError): pass
        peak = max(peak, rss)
        avail = int(next(line.split()[1] for line in pathlib.Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:')))
        pressure = float(pathlib.Path('/proc/pressure/memory').read_text().splitlines()[1].split()[1].split('=')[1])
        stall_since = (stall_since or time.monotonic()) if pressure > 5.0 else None
        if rss > limit or avail < 600 * 1024 or (stall_since and time.monotonic() - stall_since > 3) or time.monotonic() - start > 600:
            reason = f'stopped: tree_rss_kib={rss}, available_kib={avail}, full_avg10={pressure}'
            os.killpg(child.pid, signal.SIGTERM)
            child.wait()
            break
        time.sleep(.1)
    result = dict(command=cmd,limit_kib=limit,peak_tree_rss_kib=peak,elapsed_seconds=round(time.monotonic()-start,2),exit_code=child.wait(),stop_reason=reason)
    out.with_suffix('.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result))
    sys.exit(result['exit_code'] if reason is None else 125)
