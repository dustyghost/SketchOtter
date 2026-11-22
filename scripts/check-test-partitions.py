#!/usr/bin/env python3
"""Check libtest inventories; this audits coverage, never selects or rewrites tests."""
import argparse
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("complete", type=Path)
parser.add_argument("focused", type=Path)
args = parser.parse_args()


def inventory(path):
    return {line.removesuffix(": test") for line in path.read_text().splitlines()
            if line.endswith(": test")}


complete, focused = inventory(args.complete), inventory(args.focused)
assert complete and focused, "Expected nonempty libtest inventories"
assert focused <= complete, f"Focused-only tests: {sorted(focused - complete)}"
for prefix in ("app::retopology_tests::", "document::retopology::tests::",
               "sculpt::retopology::tests::"):
    expected = {name for name in complete if name.startswith(prefix)}
    assert expected, f"Missing partition: {prefix}"
    assert expected <= focused, f"Missing focused tests: {sorted(expected - focused)}"
assert any(name.endswith("::native_retopology_controls_offscreen_review")
           for name in complete), "The full harness must retain retopology GPU controls"
assert not any("offscreen" in name for name in focused), "CPU partition contains a GPU capture"
print(f"Full harness: {len(complete)}; focused: {len(focused)}; coverage inventory OK")
