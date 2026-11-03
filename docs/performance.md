# Performance report: hide/reveal speedups (Nov 2026)

This compares commit `51cecd1` (first v2 core) with `6602834` ("Speed up
hide/reveal"). The cryptography and embedding quality did not change between
them; only how the work is scheduled and computed.

## Summary

- **Release builds:** hiding is **2–4.4× faster**, revealing **2.6–4× faster**
  on photos. An 8 MP PNG now hides in 0.8 s instead of 3.6 s.
- **Debug builds (`npm run tauri dev`):** this is where the app felt slow.
  Hiding an 8 MP PNG went from **20.7 s to 1.0 s** (21×). Debug and release
  now perform almost the same.
- **Security is unchanged:** the same Argon2id parameters (64 MiB, 3 passes),
  ChaCha20-Poly1305, STC constraint heights and cost functions. The number of
  changed elements is the same as before (e.g. 138,277 vs 138,104 on the 8 MP
  PNG), and Sample Pair Analysis still reads exactly as on the clean image.
- **Costs:** peak memory is 10–18% higher when hiding and 40–60% higher when
  revealing, and revealing uses somewhat more total CPU time (spread over more
  cores). See [Trade-offs](#trade-offs).

## Setup

- Apple M4 Pro, 12 cores, 24 GB RAM, macOS; Rust 1.99.1.
- The `stegano` CLI from each commit, built with `cargo build --release` and
  with plain `cargo build` (the debug profile `tauri dev` uses).
- Carriers: a 3840 × 2160 aerial photo as JPEG (q≈90) and PNG, a 6016 × 6016
  image as JPEG and PNG, and a 10 s 44.1 kHz stereo 16-bit WAV.
- Payload: one 100 KB random file (20 KB for the WAV).
- [hyperfine](https://github.com/sharkdp/hyperfine): median of 5 runs
  (release) or 3 runs (debug), after one warm-up run. Each version reveals its
  own output (the two layouts are incompatible). Every reveal was checked
  byte-for-byte against the original payload.

## Release builds

| Carrier | Hide before | Hide after | Speedup | Reveal before | Reveal after | Speedup |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 MP JPEG | 1.73 s | 0.84 s | 2.1× | 0.78 s | 0.30 s | 2.6× |
| 36 MP JPEG | 3.32 s | 1.40 s | 2.4× | 1.88 s | 0.56 s | 3.4× |
| 8 MP PNG | 3.58 s | 0.81 s | 4.4× | 0.92 s | 0.31 s | 3.0× |
| 36 MP PNG | 10.82 s | 2.46 s | 4.4× | 4.16 s | 1.04 s | 4.0× |
| 10 s stereo WAV | 0.20 s | 0.14 s | 1.4× | 0.09 s | 0.08 s | 1.1× |

## Debug builds (`tauri dev`)

| Carrier | Hide before | Hide after | Speedup | Reveal before | Reveal after | Speedup |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 MP JPEG | 3.49 s | 0.92 s | 3.8× | 2.00 s | 0.34 s | 6.0× |
| 36 MP JPEG | 5.37 s | 1.47 s | 3.6× | 3.15 s | 0.64 s | 4.9× |
| 8 MP PNG | 20.70 s | 0.99 s | 20.9× | 2.57 s | 0.34 s | 7.6× |
| 36 MP PNG | 52.57 s | 3.06 s | 17.2× | 7.14 s | 1.18 s | 6.0× |
| 10 s stereo WAV | 1.38 s | 0.16 s | 8.8× | 1.25 s | 0.09 s | 14.7× |

## Where the time went (8 MP PNG, release, hide)

| Phase | Before | After | What changed |
| --- | ---: | ---: | --- |
| Decode carrier | 86 ms | 91 ms | — |
| Argon2id key derivation | 69 ms | 82 ms¹ | Runs in parallel with the cost map |
| Cost map (HILL) and gather | 145 ms | ¹ | Gathering moved into the STC chunks |
| Embedding order (permutation) | 640 ms | 106 ms | Serial Fisher-Yates → parallel keyed sort |
| STC (Viterbi) | 645 ms | 396 ms | Vectorizable inner loop; segments in parallel |
| Encode PNG | 1880 ms | 60 ms | zlib "High" → fdeflate (`Fast`), which is also smaller |
| Verify (re-decode) | ~80 ms | 68 ms | — |

¹ Argon2id and the cost map now run at the same time; 82 ms covers both.

The JPEG path never had the PNG encoder problem, which is why its gain is
smaller (about 2×).

## What changed

1. **Optimized dependencies in debug builds.** Only `stegano-core` used to be
   optimized in the dev profile. Argon2, ChaCha20, the PNG/JPEG/FLAC codecs and
   `rayon` ran at `-O0`, which is why `tauri dev` was 4–20× slower than release.
   `[profile.dev.package."*"] opt-level = 3` fixes that. The first debug build
   takes a little longer; later ones are incremental as usual.
2. **Keyed embedding order as a parallel sort.** Previously two serial
   Fisher-Yates shuffles over the whole cover took about 650 ms. Now every
   index gets a 32-bit tag from the ChaCha20 keystream (computed in parallel,
   since the keystream can be entered at any position) and the indices are
   sorted by tag with `rayon`'s parallel sort. Sorting by a keyed pseudorandom
   function gives a uniformly random permutation to anyone without the key,
   just as before. The public salt positions use a golden-ratio affine scatter,
   which spreads them evenly and needs no table at all.
3. **STC.** The add-compare-select step works on fixed groups of 8 states,
   specialized for each low-bit pattern of the trellis column, so the compiler
   can vectorize it. Each chunk gathers its own inputs inside the parallel
   loop, and the salt, header and body segments are embedded at the same time.
   The salt and header previously ran alone on one or two cores.
4. **PNG encoding** uses the png crate's `Fast` mode (fdeflate): 31× faster,
   and on these images the output is smaller too (11.6 MB vs 12.0 MB with the
   old setting; the original file was 12.9 MB).
5. **Overlap:** Argon2id and the cost map are independent, so they run at the
   same time with `rayon::join`.

A per-position AES-based permutation was also tried and dropped. The `aes`
crate only uses Apple Silicon's AES instructions with a special build flag,
and 32-bit Android devices have no AES hardware, so it made things slower.

## Trade-offs

**Peak memory (median, MiB):**

| Carrier | Hide before | Hide after | Reveal before | Reveal after |
| --- | ---: | ---: | ---: | ---: |
| 8 MP JPEG | 888 | 979 | 383 | 548 |
| 36 MP JPEG | 1788 | 1982 | 633 | 1043 |
| 8 MP PNG | 794 | 908 | 353 | 544 |
| 36 MP PNG | 2991 | 3543 | 1363 | 2103 |
| 10 s stereo WAV | 190 | 197 | 79 | 85 |

The increase comes from the sort: it briefly holds an 8-byte (tag, index) pair
per element, about 200 MB for an 8 MP photo, before shrinking to the 4-byte
order the old version kept. A radix sort in place, or releasing the tags
before the order is built, would bring this back down; that is the obvious
next step if phones run out of memory.

**Total CPU time:** hiding now uses less CPU overall (5.3 vs 8.3 CPU-seconds on
the 8 MP PNG), but revealing uses more (1.5 vs 1.0), because the parallel sort
does more total work than the serial shuffle it replaced. Wall-clock time is
what you wait for, and it is 3× lower.

## Reproducing

Build the CLI and time it with hyperfine, for example:

```sh
cargo build --release -p stegano-cli
export STEGANO_PASSPHRASE='benchmark passphrase'
hyperfine -N --warmup 1 --runs 5 \
  'target/release/stegano hide photo.png -o /tmp/out.png -f payload.bin'
hyperfine -N --warmup 1 --runs 5 \
  'target/release/stegano reveal /tmp/out.png -d /tmp --force'
```
