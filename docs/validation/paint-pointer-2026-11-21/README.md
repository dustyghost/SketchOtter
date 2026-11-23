# Paint stroke startup on a dense model

On the i7-7500U laptop, the median first dab of a repeated material-only stroke on 2,408,448 triangles fell from 445 to 239 ms. A first stroke after hiding faces still took 944 ms. The material edit now takes its single undo snapshot on the first Paint dab. Previously, pointer-down copied the model and the first dab immediately discarded that copy and made another. A click with no dab creates no history entry.

## Paired release measurements

Each cell shows baseline / candidate in milliseconds from pointer-down through the first completed CPU dab. Each median has four observations. Trials alternate execution order within one executable. Each arm starts with an independent masked organic fixture with hidden faces, then runs all four states in order. The brush uses varying pressure and alpha. The CSV includes each of the following three sustained dabs and separate timings for the snapshot, normals and eligibility, hierarchy validation/build/copy/query, weights and application.

| Machine | Source triangles | Cold | Repeated material stroke | After deformation | After visibility edit |
| --- | ---: | ---: | ---: | ---: | ---: |
| i7-7500U, 7.63 GiB | 602,112 | 152.60 / 121.13 | 56.09 / 44.31 | 152.25 / 133.95 | 202.95 / 192.45 |
| i7-7500U, 7.63 GiB | 2,408,448 | 936.79 / 782.29 | 445.44 / 239.35 | 861.55 / 675.56 | 1037.53 / 943.64 |
| Xeon 6975P-C, 15.33 GiB | 602,112 | 99.06 / 86.92 | 52.99 / 37.38 | 97.05 / 84.20 | 146.75 / 130.45 |
| Xeon 6975P-C, 15.33 GiB | 2,408,448 | 583.33 / 480.67 | 318.79 / 183.65 | 519.28 / 412.00 | 691.36 / 592.66 |

At 2.4m, warm sustained-dab medians were 10.04 / 9.56 ms on the laptop and 5.54 / 5.61 ms on the hosted runner. After a visibility change these were 14.67 / 14.81 ms and 9.07 / 9.85 ms. The change addresses startup. The laptop's remaining warm first-dab snapshot took 168 ms. Its cold and post-visibility hierarchy builds took 463 and 737 ms. Neither machine met an interactive first-dab target for these dense cases.

The retained history estimate matched in every paired state, growing from 271.88 MiB to 812.84 MiB at 2.4m as edits accumulated. Each arm reproduced the same document hashes across release, Undo and Redo. Whole-process high-water RSS (both arms, allocated sequentially) was 478.6 / 1821.7 MiB on the laptop and 476.9 / 1794.7 MiB on the hosted runner at 602k / 2.4m. These peaks do not isolate transient allocations per arm. The laptop 2.4m run used up to 17.8 MiB sampled swap. Per-process limits, available memory, build identifiers and toolchains are recorded in [environment.json](environment.json). Full timings, hashes and retained history are in [samples.csv](samples.csv).

## Reproduction and correctness

Build the release library tests, then use the executable Cargo prints. Use a fresh output directory for each run and select limits appropriate to the machine.

```sh
cargo test --locked --release --lib --no-run
python3 scripts/profile-paint-pointer.py "$test_binary" target/paint-l4 --level 4 \
    --repeats 4 --max-rss-mib 1950 --min-available-mib 512
```

The paired test selects the old pointer entry point or the freehand entry point while using the same paint application and occlusion code. Both invalidate geometry after deformation and visibility changes. Separate regression tests cover image and vertex painting, shared textures, cancellation, mask and material changes, same-size topology and moving occluders. The timed fixture measures vertex Paint. Display presentation is outside this measurement. The final probe name includes the standard opt-in filter so normal headless rendering checks can run independently of the benchmark environment.

[Hosted measurement run](https://github.com/dustyghost/SketchOtter/actions/runs/44957903198)
