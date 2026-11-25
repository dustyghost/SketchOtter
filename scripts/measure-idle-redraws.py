#!/usr/bin/env python3
"""Measure SketchOtter idle update counts and process CPU on Linux/Sway."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "target/release/sketchotter"
TRACE_ENV = "CLAY_IDLE_FRAME_TRACE"
WARMUP_SECONDS = 5
SAMPLE_SECONDS = 12


def cpu_ticks(pid: int) -> int:
    fields = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8").rsplit(")", 1)[1].split()
    return int(fields[11]) + int(fields[12])


def make_project(scene: str, directory: Path) -> Path | None:
    if scene == "startup":
        return None
    project = json.loads((ROOT / "tests/fixtures/v1-project.json").read_text(encoding="utf-8"))
    if scene == "empty":
        project["mesh"] = {"vertices": [], "triangles": [], "mask": []}
    else:
        project["mesh"] = json.loads(
            (ROOT / "tests/fixtures/smoothing-groove.json").read_text(encoding="utf-8")
        )
    path = directory / f"{scene}.clay"
    path.write_text(json.dumps(project), encoding="utf-8")
    return path


def measure(scene: str) -> None:
    with tempfile.TemporaryDirectory(prefix=f"sketchotter-idle-{scene}-", dir="/tmp/opencode") as temp:
        directory = Path(temp)
        project = make_project(scene, directory)
        config = directory / "config"
        data = config / "sketchotter"
        data.mkdir(parents=True)
        version = re.search(
            r'^\s*version\s*=\s*"([^"]+)"',
            (ROOT / "Cargo.toml").read_text(encoding="utf-8"),
            re.MULTILINE,
        )
        if version is None:
            raise RuntimeError("Could not read the app version from Cargo.toml")
        (data / "preferences.json").write_text(
            json.dumps({"dismissed_startup_version": version.group(1)}),
            encoding="utf-8",
        )

        env = os.environ.copy()
        env["XDG_CONFIG_HOME"] = str(config)
        env[TRACE_ENV] = "1"
        command = [str(APP)]
        if project is not None:
            command.append(str(project))
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            time.sleep(WARMUP_SECONDS)
            if process.poll() is not None:
                stdout, stderr = process.communicate()
                raise RuntimeError(f"SketchOtter exited during startup:\n{stdout}{stderr}")

            start = time.monotonic()
            before = cpu_ticks(process.pid)
            time.sleep(SAMPLE_SECONDS)
            elapsed = time.monotonic() - start
            after = cpu_ticks(process.pid)
            ticks_per_second = os.sysconf(os.sysconf_names["SC_CLK_TCK"])
            cpu = (after - before) / ticks_per_second / elapsed * 100

            close = subprocess.run(
                ["swaymsg", '[app_id="sketchotter"] kill'],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if close.returncode != 0:
                raise RuntimeError(f"Could not close the test window through Sway: {close.stderr}")
            try:
                process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=3)
            stdout, stderr = process.communicate()
            print(
                f"scene={scene} sample_seconds={elapsed:.2f} "
                f"process_cpu_percent_of_one_core={cpu:.2f} "
                f"exit_status={process.returncode}"
            )
            if stdout:
                print(stdout, end="")
            if stderr:
                print(stderr, end="")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()


def main() -> None:
    if not APP.is_file():
        raise SystemExit("Build the app first with `cargo build --locked --release -j 1`.")
    if not Path("/tmp/opencode").is_dir():
        raise SystemExit("This script expects the approved /tmp/opencode temporary directory.")
    if shutil.which("swaymsg") is None:
        raise SystemExit("This script requires Sway IPC (`swaymsg`) to close the test window.")
    existing = subprocess.run(["pgrep", "-x", "sketchotter"], capture_output=True, text=True)
    if existing.returncode == 0:
        raise SystemExit("Close existing SketchOtter windows before running this measurement.")
    for scene in ("empty", "startup", "loaded"):
        measure(scene)


if __name__ == "__main__":
    main()
