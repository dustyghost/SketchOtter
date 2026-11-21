#!/usr/bin/env python3
"""Run the opt-in Paint pointer benchmark in a bounded Linux process."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform

TEST = "viewport::gpu::profile::capacity::paint_pointer::paint_pointer_startup"
spec = importlib.util.spec_from_file_location("capacity", Path(__file__).with_name("benchmark-capacity.py"))
capacity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capacity)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path, help="path to the optimised Cargo library test executable")
    parser.add_argument("output", type=Path)
    parser.add_argument("--level", type=int, choices=range(5), required=True)
    parser.add_argument("--repeats", type=int, choices=range(1, 11), default=4)
    parser.add_argument("--max-rss-mib", type=int, required=True)
    parser.add_argument("--min-available-mib", type=int, default=512)
    parser.add_argument("--timeout", type=int, default=900)
    args = parser.parse_args()
    if min(args.max_rss_mib, args.min_available_mib) < 256 or args.timeout < 1:
        parser.error("allow at least 256 MiB for each memory limit and a positive timeout")
    output = args.output.resolve()
    binary = args.executable.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    cpu = next(line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
               if line.startswith("model name"))
    info = {
        "level": args.level, "repeats": args.repeats, "max_rss_mib": args.max_rss_mib,
        "min_available_mib": args.min_available_mib, "timeout_seconds": args.timeout,
        "executable_sha256": capacity.file_digest(binary), "source_commit": capacity.command("git", "rev-parse", "HEAD"),
        "platform": platform.platform(), "rustc": capacity.command("rustc", "--version"),
        "memory_kib": capacity.proc_kib("/proc/meminfo", "MemTotal"), "cpu": cpu,
    }
    env = dict(os.environ, CLAY_PAINT_LEVEL=str(args.level), CLAY_PAINT_REPEATS=str(args.repeats),
               CLAY_PAINT_PROFILE=str(output / "samples.json"))
    with capacity.workload_lock():
        available = capacity.proc_kib("/proc/meminfo", "MemAvailable")
        info["preflight_available_kib"] = available
        if available is None or available < (args.max_rss_mib + args.min_available_mib) * 1024:
            result = {"status": "skipped", "reason": "insufficient available RAM for the RSS budget plus reserve"}
        else:
            result = capacity.bounded_run(str(binary), env, output / "run.log", args, test_name=TEST)
    if result["status"] == "measured":
        samples = json.loads((output / "samples.json").read_text())
        if len(samples["trials"]) != args.repeats:
            raise RuntimeError("missing measured trials")
    info["result"] = result
    (output / "guard.json").write_text(json.dumps(info, indent=2) + "\n")
    print(json.dumps(info), flush=True)
    return 0 if result["status"] == "measured" else 1


if __name__ == "__main__":
    raise SystemExit(main())
