#!/usr/bin/env python3
"""Measure injected X11 pen-like mouse input to the first visible changed model pixels."""
import argparse
import hashlib
import json
import os
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

    rows = []
    for sample in range(12):
        dx = (sample % 5 - 2) * 8
        dy = (sample // 5 - 1) * 10
        move(cx+dx, cy+dy)
        time.sleep(.16)
        before = stable()
        start = time.monotonic_ns()
        xtest.fake_input(connection, X.ButtonPress, 1)
        connection.flush()
        last_unchanged = start
        first_visible = None
        observations = 0
        # Press at the established hover position. The first changed pixels
        # precede dragging, so a moving hover ring cannot supply the first sample.
        while time.monotonic_ns()-start < 2_500_000_000:
            value = capture()
            observed = time.monotonic_ns()
            observations += 1
            if value != before:
                first_visible = observed
                break
            last_unchanged = observed
            time.sleep(.001)
        move(cx+dx+9,cy+dy+4)
        time.sleep(.04)
        xtest.fake_input(connection, X.ButtonRelease, 1)
        connection.sync()
        move(width//12, height//12)
        time.sleep(.22)
        after = stable()
        rows.append({"trial":sample, "first_change_ms":(first_visible-start)/1e6 if first_visible else None,
                     "last_unchanged_ms":(last_unchanged-start)/1e6,"polls":observations,
                     "before_sha256":digest(before),"after_release_digest":digest(after)})
    good = [r["first_change_ms"] for r in rows if r["first_change_ms"] is not None]
    persistent = len({r["after_release_digest"] for r in rows})
    report = {"screen_pixels":[screen.width_in_pixels,screen.height_in_pixels],"window_pixels":[width,height],
              "sample_region_pixels":size,"window_origin":[origin.x,origin.y],"distinct_released_images":persistent,
              "timed_changes":len(good),"median_visible_change_ms":statistics.median(good) if good else None,
              "display":os.environ["DISPLAY"],"x_server":subprocess.check_output(["xdpyinfo"],text=True).split("number of extensions:")[0][-800:],
              "measurement":"XTest input flush to first changed pixels read from X11 window; excludes physical tablet, monitor and scan-out",
              "rows":rows}
    args.output.write_text(json.dumps(report,indent=2)+"\n")
    print("DESKTOP_RECORD",json.dumps(report,separators=(",",":")),flush=True)
    if len(good)<8 or persistent<3:
        raise RuntimeError("too few visible edits to establish a reliable latency series")


if __name__ == "__main__":
    main()
