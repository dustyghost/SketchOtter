#!/usr/bin/env python3
"""Run one fixed-topology comparison or a mixed session under Linux memory guards."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform

spec = importlib.util.spec_from_file_location("capacity", Path(__file__).with_name("benchmark-capacity.py"))
capacity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capacity)
CASES = {
    "picking": "picking::gpu_load_profile_world_picking",
    "drawing": "drawing::gpu_load_profile_dense_materials",
    "visual": "release::gpu_load_profile_release_visual",
    "release": "release::gpu_load_profile_release_fans",
    "display": "regions::gpu_load_profile_changed_regions",
    "boundary": "session::gpu_load_profile_upper_boundary",
    "session": "session::gpu_load_profile_mixed_session",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("workload", choices=CASES)
    parser.add_argument("output", type=Path)
    parser.add_argument("--level", type=int, choices=(3, 4), required=True)
    parser.add_argument("--asset", type=Path, help="directory produced by prepare-capacity-asset.py")
    parser.add_argument("--max-rss-mib", type=int, required=True)
    parser.add_argument("--min-available-mib", type=int, default=1024)
    parser.add_argument("--timeout", type=int, default=1800)
    args = parser.parse_args()
    if min(args.max_rss_mib, args.min_available_mib) < 256 or args.timeout < 1:
        parser.error("memory limits must each be at least 256 MiB and timeout must be positive")
    if args.workload == "session" and (args.asset is None or not (args.asset / "WaterBottle.obj").is_file()):
        parser.error("the session requires a prepared Water Bottle in --asset")
    executable = args.executable.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, CLAY_REGION_LEVEL=str(args.level), CLAY_REGION_OUTPUT=str(out / "samples.json"))
    if args.asset:
        env["CLAY_REGION_ASSET"] = str(args.asset.resolve())
    test = "viewport::gpu::profile::capacity::" + CASES[args.workload]
    with capacity.workload_lock():
        available = capacity.proc_kib("/proc/meminfo", "MemAvailable")
        if available is None or available < 1024 * (args.max_rss_mib + args.min_available_mib):
            result = {"status": "skipped", "reason": "available RAM below process budget plus reserve"}
        else:
            result = capacity.bounded_run(str(executable), env, out / "case.log", args, test_name=test)
    if result["status"] == "measured" and not (out / "samples.json").is_file():
        raise RuntimeError("no measurements written by the selected test")
    info = {"workload": args.workload, "level": args.level, "max_rss_mib": args.max_rss_mib,
            "min_available_mib": args.min_available_mib, "preflight_available_kib": available,
            "total_kib": capacity.proc_kib("/proc/meminfo", "MemTotal"), "platform": platform.platform(),
            "test_binary_sha256": capacity.file_digest(executable), "commit": capacity.command("git", "rev-parse", "HEAD"),
            "result": result}
    (out / "environment.json").write_text(json.dumps(info, indent=2) + "\n")
    print(json.dumps(result))
    return 0 if result["status"] == "measured" else 1


if __name__ == "__main__":
    raise SystemExit(main())
