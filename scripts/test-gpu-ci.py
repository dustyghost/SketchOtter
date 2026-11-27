#!/usr/bin/env python3
"""Selection regressions: CI must fail instead of silently running zero tests."""
import contextlib
import importlib.util
import io
import json
import os
import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("gpu_ci", Path(__file__).with_name("run-gpu-ci.py"))
gpu_ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gpu_ci)


class SelectionTests(unittest.TestCase):
    inventory = ["viewport::gpu::tests::pixels", "viewport::gpu::profile::gpu_load_profile",
                 "app::tests::controls", "app::tests::more_controls"]

    def test_exact_suffix_and_automatic_core_coverage(self):
        result = gpu_ci.select_tests(self.inventory, {"a": ["controls"], "b": ["more_controls"]})
        self.assertEqual(result, {"a": ["app::tests::controls"],
                                  "b": ["app::tests::more_controls"],
                                  "core": ["viewport::gpu::tests::pixels"]})

    def test_missing_renamed_or_nonignored_test_fails(self):
        with self.assertRaisesRegex(ValueError, "expected one ignored test"):
            gpu_ci.select_tests(self.inventory, {"a": ["removed_controls"]})

    def test_ambiguous_name_fails(self):
        with self.assertRaisesRegex(ValueError, "expected one ignored test"):
            gpu_ci.select_tests(self.inventory + ["app::other::controls"], {"a": ["controls"]})

    def test_duplicate_selection_fails(self):
        for groups in ({"a": ["controls", "controls"]},
                       {"a": ["controls"], "b": ["controls"]}):
            with self.subTest(groups=groups), self.assertRaisesRegex(ValueError, "Duplicate"):
                gpu_ci.select_tests(self.inventory, groups)

    def test_empty_interaction_group_fails(self):
        with self.assertRaisesRegex(ValueError, "Empty interaction group"):
            gpu_ci.select_tests(self.inventory, {"a": []})

    def test_empty_core_fails(self):
        with self.assertRaisesRegex(ValueError, "Missing core"):
            gpu_ci.select_tests(["app::tests::controls"], {"a": ["controls"]})

    def test_core_overlap_fails(self):
        with self.assertRaisesRegex(ValueError, "also selected"):
            gpu_ci.select_tests(self.inventory, {"a": ["pixels"]})


class RunnerTests(unittest.TestCase):
    def run_fixture(self, directory, failure=False):
        directory = Path(directory)
        manifest = directory / "selection.json"
        names = ["controls", "native_retopology_controls_offscreen_review"]
        manifest.write_text(json.dumps({"a": names}))
        artifact = {"reason": "compiler-artifact", "executable": "/fake/harness",
                    "target": {"kind": ["lib"]}, "profile": {"test": True}}
        build = subprocess.CompletedProcess([], 0, json.dumps(artifact))
        outcomes = [build, subprocess.CompletedProcess([], 1 if failure else 0),
                    subprocess.CompletedProcess([], 0)]
        listing = "viewport::gpu::tests::pixels: test\n" + "".join(
            f"app::tests::{name}: test\n" for name in names)
        summary = directory / "summary.md"
        with patch.object(gpu_ci, "MANIFEST", manifest), \
                patch("sys.argv", ["run-gpu-ci.py", "a"]), \
                patch.dict(os.environ, {"RUNNER_TEMP": str(directory),
                                        "GITHUB_STEP_SUMMARY": str(summary),
                                        "CLAY_GUIDE_SKIP_IMAGES": "1"}), \
                patch.object(gpu_ci.subprocess, "run", side_effect=outcomes) as run, \
                patch.object(gpu_ci.subprocess, "check_output", return_value=listing), \
                contextlib.redirect_stdout(io.StringIO()):
            if failure:
                with self.assertRaises(subprocess.CalledProcessError):
                    gpu_ci.main()
            else:
                gpu_ci.main()
        results = json.loads((directory / "gpu-review/timings-a.json").read_text())
        return run.call_args_list, results, summary.read_text()

    def test_build_once_exact_execution_and_retained_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            calls, results, summary = self.run_fixture(directory)
        self.assertEqual(len(calls), 3)
        self.assertIn("--no-run", calls[0].args[0])
        self.assertEqual(calls[1].args[0], ["/fake/harness", "app::tests::controls",
                                         "--exact", "--ignored", "--test-threads=1"])
        self.assertEqual(calls[1].kwargs["env"]["CLAY_GUIDE_SKIP_IMAGES"], "1")
        self.assertNotIn("CLAY_GUIDE_SKIP_IMAGES", calls[2].kwargs["env"])
        self.assertEqual(len(results), 2)
        self.assertIn("pass", summary)

    def test_failure_stops_group_and_retains_timing(self):
        with tempfile.TemporaryDirectory() as directory:
            calls, results, summary = self.run_fixture(directory, failure=True)
        self.assertEqual(len(calls), 2)
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0]["exit_code"], 1)
        self.assertIn("FAIL", summary)


if __name__ == "__main__":
    unittest.main()
