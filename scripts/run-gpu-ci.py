#!/usr/bin/env python3
"""Run the audited GPU selection serially, with one harness build per runner."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

MANIFEST = Path(__file__).with_name("gpu-ci-tests.json")


def select_tests(inventory, groups):
    """Resolve every selector exactly once; reject missing/overlapping coverage."""
    selected = {}
    owners = {}
    for group, selectors in groups.items():
        if not selectors:
            raise ValueError(f"Empty interaction group: {group}")
        selected[group] = []
        for selector in selectors:
            matches = [name for name in inventory if name.endswith("::" + selector)]
            if len(matches) != 1:
                raise ValueError(f"{selector}: expected one ignored test, found {matches}")
            name = matches[0]
            if name in owners:
                raise ValueError(f"Duplicate selection: {name} in {owners[name]} and {group}")
            owners[name] = group
            selected[group].append(name)
    selected["core"] = sorted(name for name in inventory
                              if name.startswith("viewport::gpu::")
                              and "gpu_load_profile" not in name)
    if not selected["core"]:
        raise ValueError("Missing core GPU tests")
    if set(selected["core"]) & owners.keys():
        raise ValueError("Core GPU tests also selected by an interaction group")
    return selected


def main():
    groups = json.loads(MANIFEST.read_text())
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("group", choices=["core", *groups])
    args = parser.parse_args()
    # Cargo JSON keeps diagnostics visible on stderr and identifies the harness
    # without depending on target directory/profile naming or a glob.
    build = subprocess.run(
        ["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"],
        text=True, stdout=subprocess.PIPE,
    )
    messages = [json.loads(line) for line in build.stdout.splitlines()]
    for entry in messages:
        if entry.get("reason") == "compiler-message":
            print(entry["message"].get("rendered", ""), file=sys.stderr, end="")
    build.check_returncode()
    binaries = [entry["executable"] for entry in messages
                if entry.get("reason") == "compiler-artifact"
                and entry.get("executable") and entry["target"]["kind"] == ["lib"]
                and entry["profile"]["test"]]
    if len(binaries) != 1:
        raise ValueError(f"Expected one libtest harness: {binaries}")
    binary = binaries[0]
    listing = subprocess.check_output([binary, "--list", "--ignored"], text=True)
    inventory = [line.removesuffix(": test") for line in listing.splitlines()
                 if line.endswith(": test")]
    selected = select_tests(inventory, groups)
    print("GPU selection audit: " + ", ".join(
        f"{group}={len(names)}" for group, names in selected.items()), flush=True)
    directory = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / "gpu-review"
    directory.mkdir(parents=True, exist_ok=True)
    results = []
    try:
        for name in selected[args.group]:
            print(f"::group::{name}", flush=True)
            env = os.environ.copy()
            env["CLAY_GUIDE_PREVIEW"] = str(directory / name.split("::")[-1])
            # Preserve the Integer grid images already retained by CI. Other
            # Guide screenshots remain available through manual cargo commands.
            if name.endswith("::native_retopology_controls_offscreen_review"):
                env.pop("CLAY_GUIDE_SKIP_IMAGES", None)
            else:
                env["CLAY_GUIDE_SKIP_IMAGES"] = "1"
            start = time.monotonic()
            result = subprocess.run(
                [binary, name, "--exact", "--ignored", "--test-threads=1"], env=env,
            )
            elapsed = time.monotonic() - start
            results.append({"test": name, "seconds": round(elapsed, 3),
                            "exit_code": result.returncode})
            print(f"{name}: {elapsed:.2f}s\n::endgroup::", flush=True)
            result.check_returncode()
    finally:
        path = directory / f"timings-{args.group}.json"
        path.write_text(json.dumps(results, indent=2) + "\n")
        if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(summary, "a") as output:
                output.write(f"### GPU timings: {args.group}\n\n")
                output.write("| Test | Seconds | Result |\n|---|---:|---|\n")
                for row in sorted(results, key=lambda row: -row["seconds"]):
                    status = "pass" if row["exit_code"] == 0 else "FAIL"
                    output.write(f"| {row['test']} | {row['seconds']:.2f} | {status} |\n")


if __name__ == "__main__":
    main()
