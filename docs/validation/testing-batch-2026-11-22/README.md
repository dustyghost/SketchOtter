# Focused testing and mixed edit sequences, 22 November 2026

Base: `67b1352` (PR #187). Issues #176 and #175. The two user session folders,
installed release app and shared build caches were preserved. No release build
was run.

## Compilation memory (#176)

Host: mark-laptop, 7.6 GiB usable RAM. Compiler: rustc 1.99.0
(`b940084d7`, LLVM 23.1.1), x86_64 Linux. Test profile: unoptimized application,
no debug information, incremental disabled; native solver/BatSat/glam retain
the repository's optimized dependency settings. One Cargo job, one test thread.
The final comparison sets the application crate's codegen-units to 256 for all
three workflows. Dependencies came from the existing development cache.

[The retained probe](memory-probe.py) samples process-group RSS every 100 ms,
terminates **only its own command group** at the selected budget, at less than
600 MiB available host RAM, or after sustained full-memory PSI above 5% for
three seconds. Each command also has a 600-second ceiling. These are sampled
process-tree peaks (Cargo/compiler/linker), not exact allocator peaks or a hard
cgroup allocation limit. Existing host build processes were checked separately;
commands ran serially under `/home/mark/.cache/codex-build.lock`.

| Workflow | Outcome | Sampled peak MiB | Seconds | Record |
| --- | --- | ---: | ---: | --- |
| Default harness, test-name filter, no-run | Stopped at 1,800 MiB; compilation incomplete | 1,801 | 38.94 | [JSON](default-1800.json) |
| Old source-rewriting staging script, 18 selected cases | Compiled | 1,628 | 44.22 | [JSON](staged-pinned.json) |
| Supported working-source partition, 22 cases including shared instrumentation | Compiled | 1,439 | 37.30 | [JSON](focused-1800.json) |

The default and staged commands use exactly the old test bodies; the partition
only moves the five app tests and three group tests and gates unrelated test
modules. A source inventory comparison found **1,479 test bodies before and
1,479 after, with no added, removed or duplicated test names** in #176. The old
selection is retained in [this inventory](prior-staging-inventory.txt).

The smaller trial that only excluded the large general app-test module crossed
the initial 1,200 MiB ceiling ([record](app-partition.json)), as did the full
harness ([record](baseline-final.json)). Test-module gates were chosen instead
of splitting the runtime application or exposing private internals. The focused
partition is about 189 MiB below the old staging workflow in this comparison.
It still does **not** fit the historical 1,400 MiB cap: the current source peaked
39 MiB above it. Use the declared 1,800 MiB process-tree budget only when the
host has at least that budget plus the reserve available; defer when it does not.
No swap was added, resized or treated as compiler capacity.

Initial cold dependency attempts were stopped for memory pressure and are not
harness measurements. The staging script also omitted `rust-toolchain.toml`, so
an initial comparison selected the host's Rust 1.94.1 and was stopped; the valid
comparison explicitly uses `cargo +1.99.0`. Both are reasons to use the supported
working-source command rather than maintain a copied tree.

Reproduce (set `CARGO_TARGET_DIR` to an existing development cache if desired):

```sh
free -h
cat /proc/pressure/memory
# Inspect host cargo/rustc/compiler processes before starting.
flock -n /home/mark/.cache/codex-build.lock env CLAY_PROBE_LIMIT_MIB=1800 \
  python3 docs/validation/testing-batch-2026-11-22/memory-probe.py /tmp/focused-tests \
  env CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false \
  cargo test --locked -j 1 --lib --features focused-retopology -- --test-threads=1
```

The prefix produces a `.json` measurement and `.log` command output. Full test,
format/lint and GPU correctness checks are left to PR CI. The focused command
runs the existing integration tests from the working tree; GPU/control captures
remain in the default harness and graphics CI. See [testing](../../testing.md)
for the moved-test and partition inventory.

The supported command passed **18 tests**, with four pre-existing manual tests
ignored, in 16.32 seconds of test execution. Compile plus execution took 48.17
seconds and peaked at 1,453 MiB ([measurement](focused-tests.json),
[log](focused-tests.txt)). The extra four ordinary checks and three ignored
profiles belong to shared normal/instrumentation helpers; the fourteen ordinary
retopology/group contracts and one manual adapter are all present.
