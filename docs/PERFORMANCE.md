# Terrain and viewer performance

The optimization pass keeps block precision, biome selection, shading, contours and
surface-fluid predictions. Generation and save reads remain in the isolated Rust
engine process. No Minecraft chunks are generated or modified by the benchmarks.

## Measured release-engine comparison

On the Ryzen 9 7900X3D, all **22 tile payloads are byte-identical** to the frozen
pre-optimization binary. Individual timings from the same workload matrix:

| Workload | Before | After | Speedup |
| --- | ---: | ---: | ---: |
| Close saved volcano, first tile | 0.828 s | 0.295 s | 2.81× |
| Medium regional view, first tile | 5.220 s | 1.267 s | 4.12× |
| Far detail, first tile | 12.109 s | 3.181 s | 3.81× |
| Medium view, change to rainfall | 5.138 s | 0.192 s | 26.69× |
| Far detail, change to rainfall | 10.678 s | 0.234 s | 45.56× |
| Moon, change layer | 0.692 s | 0.014 s | 48.65× |

Moon and Beneath first-tile times are essentially unchanged; the strongest first-load
gain is in Overworld terrain. These are individual observations, not statistical
averages or full-viewport/native-GPU frame times. The [complete report](data/terrain-performance-2026-10-24.json)
includes every workload, payload hash, byte count and binary digest.

## Changes

- Terrain blending generates only the quart samples and interpolation kernels a height
  query actually uses, caching them per chunk. Zero-weight kernels are skipped. The
  previous implementation remains a test oracle for exact floating-point comparison.
- Volcanic coverage caches signed 16×16 query chunks, including empty results. Feature
  candidates are enumerated once per chunk. Flood filling requests terrain heights as
  it explores; fissures and spouts also request only the columns they touch.
- Completed biome, water, volcanic-surface and relief grids are shared between engine
  workers. Locks cover cache lookup/publication, never generation. Keys retain seed,
  every generation setting, dimension, slice, source identity and reload revision.
- Relief heights also feed biome and water sampling. Overworld quart biomes and
  planetary terrain heights reuse bounded world-local caches.
- Saved surface extraction scans high sections first, resolves only unfinished
  columns and borrows packed block arrays. It retains the original handling of
  malformed records and duplicate section-Y entries. Slice/band semantics are unchanged.
- Unchanged tile plans are not repeatedly cloned and sent. Completion and skipped
  replies permit retries, including tiles dropped from an overflowing upload queue.
  Marker refreshes coalesce per poll, and structure bounds have a spatial index.
- Surface batches check cancellation between chunk reads. Worker count uses at most
  half the available logical CPUs, capped at eight, with the existing low process
  priority. Cache capacities and queues remain bounded.

## Surface context reuse follow-up

Density-column inputs now reuse the existing seed/settings-qualified LRU before
recomputing terrain height; the river color annotation reads a scalar without
cloning biome weights. Dry categorical lake pixels also avoid procedural soil
generation. No additional long-lived ground-output cache was retained.

The final four material engine cases take 6–21% less time than the prior binary,
with identical complete payload hashes. Individual runs vary; these are engine
generation/IPC timings, not a native frame-rate claim. See the
[surface follow-up](SURFACE-FOLLOWUP.md) for before/after/repeat measurements and
the separate 22-case rendering matrix.

## Reproduction and scope

`native/terralens-rs/tools/performance/engine_tiles.py` uses the normal release engine
and Python's standard library. It does not enable diagnostic commands in the app.

```sh
python native/terralens-rs/tools/performance/engine_tiles.py /path/to/old/terralens \
  --world /path/to/seed-zero-world --extended --output /tmp/before.json
python native/terralens-rs/tools/performance/engine_tiles.py \
  native/terralens-rs/target/release/terralens \
  --world /path/to/seed-zero-world --extended \
  --compare /tmp/before.json --output /tmp/after.json
```

Each case starts a fresh child engine, renders a tile, acknowledges it, then changes
the layer. The hash covers the complete RGBA pixels, biome IDs and water mask, including
tile gutters. The extended matrix includes close saved water/lava/magma, seed-only
terrain, medium and far zooms, unshaded views, Moon, Mars and Beneath. A mismatch fails
the comparison. These timings include engine work and IPC; they exclude native GPU
presentation. Fresh engine caches do not imply an empty operating-system file cache.

The separate `streaming` diagnostic example exercises the actual asynchronous viewer
at 1440×900, with panning, zooming, texture uploads and a revisit. Its frame times
measure egui submission, not native presentation or desktop responsiveness. Extreme
new-area exploration still has to generate the region and terrain data it needs.

The first optimized four-worker pass versus the final lazy-caldera/eight-worker pass
in that same scenario measured 7.24→3.25 seconds for initial refinement, 1.60→0.76
seconds to settle after panning, and 103.0→58.5 seconds for the deliberately extreme
far-area refinement. The final UI submission P95 was 0.079 ms and maximum 0.251 ms;
returning to the cached origin took 0.040 ms. These viewer comparisons do not use the
original frozen binary and must not be presented as its before/after frame times.
They also show the remaining cost of generating a very large previously unseen area.

Validation includes exact height-weight comparisons against the previous algorithm,
independent saved-height and surface fixtures, eager-versus-lazy caldera comparisons,
cross-worker grid reuse, scheduler retry recovery and saved-chunk extraction parity.

## Optional terrain profiles

Fast biome tiles use a separate precomputed palette and cached biome grid. They
never allocate a per-pixel full probe or query terrain height, cave/carver columns,
volcanic surfaces, springs or ground decoration. Full/Custom routes carry explicit
sampling needs into material, water and relief paths. Immutable biome grids are
keyed by boundary precision; physical masks and relief additionally include the
relevant terrain switches. Complete structure/ore scans survive these style changes.

A fresh two-worker release engine comparison measured:

| View | Previous basic Biomes | Fast Biomes | Speedup |
| --- | ---: | ---: | ---: |
| Local volcanic tile | 0.799 s | 0.174 s | 4.58× |
| Medium regional tile | 3.183 s | 0.384 s | 8.28× |
| Continental overview | 1.682 s | 1.215 s | 1.38× |

This is generation/IPC tile timing, not GPU frame rate. The overview still spends
time generating the underlying regional biome graph. Water geometry and block-edge
precision differ intentionally in Fast mode. Six full-terrain control payloads
match the previous application byte for byte. The new optional litter layer is
excluded from that historical-output comparison. See
[measurements](data/terrain-profile-performance.json) and
[comparison](data/terrain-profile-comparison.json).

Fast raster/export and basic hover regression tests instrument sampler calls and
require zero height, volcanic and cave-column queries. Explicit destination copies
retain resolved physical height. Cached empty volcanic masks are independently
checked to avoid repeating their source queries on a layer switch.
