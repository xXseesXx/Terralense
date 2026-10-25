# Lake colors, plateau surfaces and generation reuse

## Accepted changes

Dry lake-biome columns now use the same dark blue as covered mountain rivers in
the biome map. Previously they implicitly displayed ground materials even with
Grass colors disabled. Explicit Ground/Grass views, actual exposed water, other
thematic layers, colorblind palettes and highlighting retain their separate roles.
Categorical lake pixels consult saved substrate for fluid corrections without
generating procedural soil decorations just to choose their biome swatch.

Rocky Plateau was missing its dedicated `RockyPlateauSurfaceBuilder` dispatch.
The source compares `startY - 2` with `BiomeNoise.hills(seed,22,32)` and uses raw
host rock above that threshold. The manager's `startY` is one above the solid
cap. The Rust sampler now applies that branch using its physical builder cap,
instead of falling through to generic grass. Its hill field uses the original
seed, four octaves, `0.05f` frequency and the 85–95 elevation range. Raw top,
middle and lower states retain raw rock even when slope skips surface layers.

Two independent complete World 0 chunks test both sides of the gate:

| Chunk | Saved natural surface | Before | After |
| --- | --- | ---: | ---: |
| 1550,704 | 256 raw rhyolite columns | 0/256 | 256/256 |
| 1536,704 | 242 coarse sandy-loam dirt, nine raw and five cobble rhyolite | 256/256 | 256/256 |

The reference region is `r.48.22.mca`, SHA-256
`a67651b75347972eb02c3c8fad4c6e8f94b7810cbaea00ba917edab4bff250bd`.
The independent Python extractor reads block palettes beneath vegetation and
checks the region hash before/after. These are additional regression controls;
they do not change the fixed 48-chunk audit's denominator or 95.33% result.

Terrain density queries now reuse an already-populated column context when
seed/settings match. The river display classifier reads the cached cave weight
without cloning the vector of blended biomes. Neither optimization changes
density, material, biome, fluid or placement rules. A 16,384-entry ground-output
cache was also measured and removed: its small additional improvement did not
justify its memory cost.

Single release-engine measurements on the same material workloads:

| Workload | Before | Final | Reduction |
| --- | ---: | ---: | ---: |
| Close volcanic biome + grass | 2.356 s | 1.979 s | 16.0% |
| Change that tile to Ground | 1.500 s | 1.182 s | 21.2% |
| Medium regional biome + grass | 10.485 s | 9.419 s | 10.2% |
| Change that tile to Ground | 7.969 s | 7.501 s | 5.9% |

Every RGBA/biome/water payload in these four material cases is byte-identical.
Two additional runs without the discarded output cache measured the medium
Ground switch at 6.908–6.961 s; these are noisy single-run timings, not a
statistical benchmark or GPU frame-rate measurement. The
[before/final/repeat report](data/lake-plateau-material-comparison.json) retains
timings and binary/payload hashes. The broader 22-case matrix also completed:
seven biome payloads change with the lake display correction, and the other 15
payloads match the old binary. See [all engine cases](data/lake-plateau-engine-performance.json).

## Volcanic grass remains unresolved

The material renderer recognizes `tfg:grass/mollisol` and applies the TFC climate
tint. The reproduced error is earlier: the **seed-only sampler chooses basalt**
where the saved chunk has volcanic grass. At seed 0 **-2767,4513**, the saved
ground is grass/mollisol at Y97, on hardened basalt Y93–96, then raw dacite
Y85–92. The model's centered builder selects raw basalt at Y97; its delegated
shield builder would select grass/mollisol there. The wider fixed ground fixture
contains 76 grass→basalt mismatches in the active-shield chunk (-173,282).

The original Core 0.9.25 source and installed jar bytecode agree: cinder surface
placement requires the source cell's biome flag, rarity, easing above 0.6 and
`startY > rockHeight + heightNoise`. The traced cell is centered at approximately
(-2754.073,4513.178), has easing 0.941136 and height noise -0.247741. Both the
current gate and small one-block threshold changes select basalt. The manager
initializes the per-chunk centered-feature context; the later SURFACE callback
places bedrock, without repeating the parent surface builder.

This does not establish which pass wrote the observed grass cap. Underlying
basalt is not proof by itself: both the centered and shield builders can place
basalt strata. The world-level generator-version marker is also not proof of a
particular chunk's generation history. No seed-specific exception, blanket
basalt recoloring or speculative threshold adjustment is applied. Actual saved
substrate remains authoritative in local Ground/Grass views; seed-only volcanic
surface parity is still incomplete.

A normal release-engine export at that exact column confirms the distinction:
seed-only Grass colors gives `(56,60,62)` while opening World 0 gives the saved
grass color `(62,94,48)`, with relief disabled in both. This reproduces a surface
prediction error, rather than a missing grass texture tint.

The inspected jar is `TerraFirmaGreg-Core-Modern-0.9.25.jar`, SHA-256
`fdf9a3521a08870eae8f43a5aea1c5ea2f8e07884f516445837c0fdc2a648ec1`.
The pinned source is commit `f4929e738e83dbc8d46680276faa67520dffa172`.

An early clay-disc replay was also rejected: it increased the fixed surface
failures from 573 to 675. Its source-data and candidate-origin controls remain
test-only; release rendering does not apply those unverified patches.

## Validation

All **516 release tests** pass, including plain/stacked/colorblind lake and river
colors, explicit grass composition, dry banks, saved elevated water, the two
plateau controls and existing terrain/structure/engine regressions. A headless
library build passes. The unchanged 48-chunk audit retains 11,684/12,257 exact
surface columns and 12,231/12,257 correct fluid classes; height, climate, biome,
sandstone and leaf metrics are unchanged. Saves were read without modification;
Minecraft and the user's UI were not started or restarted.

Reproduce the two new surface controls with:

```sh
cd native/terralens-rs
cargo test --release --test rocky_plateau
python tools/reference/extract_rocky_plateau.py /path/to/world --output /tmp/plateau
python tools/performance/engine_tiles.py target/release/terralens \
  --materials-only --output /tmp/materials.json
```
