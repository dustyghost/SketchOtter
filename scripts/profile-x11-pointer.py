#!/usr/bin/env python3
"""Measure injected X11 pen-like mouse input to the first visible changed model pixels."""
import argparse
import threading
import hashlib
import json
import os
import platform
import shutil
from pathlib import Path
import statistics
import subprocess
import time


def main():
    from Xlib import X, display
    from Xlib.ext import xtest

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--window", type=int, required=True, help="SketchOtter's X window ID")
    parser.add_argument("--present-helper", type=Path, help="compiled profile-x11-present observer")
    parser.add_argument("--executable", type=Path, help="native binary from this run")
    args = parser.parse_args()
    connection = display.Display()
    screen = connection.screen()
    win = connection.create_resource_object("window", args.window)
    geo = win.get_geometry()
    origin = screen.root.translate_coords(win, 0, 0)
    width, height = geo.width, geo.height
    cx, cy = width // 2 + width // 32, height // 2
    size = min(width, height) // 5
    def move(x, y):
        xtest.fake_input(connection, X.MotionNotify, x=origin.x+x, y=origin.y+y)
        connection.sync()

    def capture():
        # Reading the client surface excludes the composited system pointer.
        pixels = win.get_image(cx-size//2, cy-size//2, size, size, X.ZPixmap, 0xffffffff)
        return pixels.data

    def digest(value):
        return hashlib.sha256(value).hexdigest()

    def stable():
        previous = capture()
        for _ in range(30):
            time.sleep(.035)
            current = capture()
            if current == previous:
                return current
            previous = current
        raise RuntimeError("the model region did not settle before input")

    events = []
    helper = None
    if args.present_helper:
        helper = subprocess.Popen([str(args.present_helper.resolve()), str(args.window)],
                                  stdout=subprocess.PIPE, text=True, bufsize=1)
        if helper.stdout.readline().strip() != "READY":
            raise RuntimeError("X Present subscription failed")
        def record_present():
            for message in helper.stdout:
                ust, serial, msc, received = map(int, message.split())
                events.append((ust * 1000, serial, msc, received))
        threading.Thread(target=record_present, daemon=True).start()
    rows = []
    for sample in range(12):
        dx = (sample % 5 - 2) * 8
        dy = (sample // 5 - 1) * 10
        move(width//12, height//12)
        time.sleep(.18)
        clean = stable()
        move(cx+dx, cy+dy)
        time.sleep(.16)
        before = stable()
        start = time.monotonic_ns()
        xtest.fake_input(connection, X.ButtonPress, 1)
        connection.flush()
        # Include a short, fixed drag so the normal desktop gesture recogniser
        # receives a stroke. Correlate frames with the final model below.
        move(cx+dx+7, cy+dy+3)
        observations = 0
        transitions = []
        previous = before
        last_read = start
        while time.monotonic_ns()-start < 1_600_000_000:
            value = capture()
            observed = time.monotonic_ns()
            observations += 1
            if value != previous:
                transitions.append((last_read, observed, value))
                previous = value
            last_read = observed
            time.sleep(.002)
        xtest.fake_input(connection, X.ButtonRelease, 1)
        connection.sync()
        move(width//12, height//12)
        time.sleep(.65)
        after = stable()
        # Select changed model pixels with the same value after brush release
        # and pointer removal. Exclude both the starting hover and clean image.
        clean_pixels = memoryview(clean).cast("I")
        before_pixels = memoryview(before).cast("I")
        after_pixels = memoryview(after).cast("I")
        changed = [i for i, pixel in enumerate(after_pixels)
                   if pixel != clean_pixels[i] and pixel != before_pixels[i]]
        last_unchanged = last_read
        first_visible = None
        for preceding, observed, value in transitions:
            pixels = memoryview(value).cast("I")
            if sum(pixels[i] == after_pixels[i] for i in changed) >= 8:
                last_unchanged = preceding
                first_visible = observed
                break
        # Only claim a content-correlated completion if one server timestamp
        # falls between the last unchanged read and first changed read. Keep
        # multiple or absent events explicitly ambiguous.
        matching = [e for e in events if first_visible and last_unchanged <= e[0] <= first_visible]
        rows.append({"trial":sample, "matched_present_ms":(matching[0][0]-start)/1e6 if len(matching)==1 else None,
                     "present_events_in_pixel_interval":len(matching), "first_change_ms":(first_visible-start)/1e6 if first_visible else None,
                     "last_unchanged_ms":(last_unchanged-start)/1e6,"polls":observations,
                     "persistent_model_pixels":len(changed),"before_sha256":digest(before),"after_release_digest":digest(after)})
    if helper:
        helper.terminate()
        helper.wait(timeout=5)
    good = [r["first_change_ms"] for r in rows if r["first_change_ms"] is not None]
    persistent = len({r["after_release_digest"] for r in rows})
    qualified = [r["matched_present_ms"] for r in rows if r["matched_present_ms"] is not None]
    report = {"server_present_events":len(events),"matched_content_presentations":len(qualified),
              "matched_presentation_median_ms":statistics.median(qualified) if qualified else None,
              "screen_pixels":[screen.width_in_pixels,screen.height_in_pixels],"window_pixels":[width,height],
              "sample_region_pixels":size,"window_origin":[origin.x,origin.y],"distinct_released_images":persistent,
              "timed_changes":len(good),"median_visible_change_ms":statistics.median(good) if good else None,
              "display":os.environ["DISPLAY"],"x_server":subprocess.check_output(["xdpyinfo"],text=True).split("number of extensions:")[0][-800:],
              "measurement":"XTest press and short drag to the first X11 read containing at least eight changed pixels confirmed after release and pointer removal; excludes physical tablet and scan-out",
              "rows":rows}
    if args.executable:
        with args.executable.open("rb") as stream:
            binary_hash = hashlib.file_digest(stream, "sha256").hexdigest()
        machine = {"platform": platform.platform(), "executable_sha256": binary_hash,
                   "backend": os.environ.get("WGPU_BACKEND"), "software_threads": os.environ.get("LP_NUM_THREADS"),
                   "cpu": next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                                if line.startswith("model name")), None),
                   "total_memory_kib": int(next(line for line in Path("/proc/meminfo").read_text().splitlines()
                                                if line.startswith("MemTotal:")).split()[1])}
        if shutil.which("vulkaninfo"):
            summary = subprocess.run(["vulkaninfo", "--summary"], capture_output=True, text=True, timeout=20)
            if summary.returncode == 0:
                machine["vulkan_devices"] = [line.strip() for line in summary.stdout.splitlines()
                                            if line.strip().startswith(("deviceName", "deviceType", "driverName", "driverInfo"))]
        report["environment"] = machine
    args.output.write_text(json.dumps(report,indent=2)+"\n")
    print("DESKTOP_RECORD",json.dumps(report,separators=(",",":")),flush=True)
    if len(good)<8 or persistent<3:
        raise RuntimeError("too few visible edits to establish a reliable latency series")


if __name__ == "__main__":
    main()
