# Generator accuracy

The seed sampler is now compared with a deterministic spread of generated World 0
chunks. The compact independent reference contains **48 complete chunks**, **12,288
columns**, 46 region-file fingerprints and 29 surface-biome IDs. Chunk slots are
ranked by SHA-256 of their signed coordinates; the first 48 full TFC chunks are
retained. This is reproducible and covers many regions, but it still measures the
places already generated in this save. Player exploration decides where those
chunks exist, so the result is not a random sample of the whole world.

| System | Result on this sample | Target status |
| --- | ---: | --- |
| Surface quart biome | 12,288 / 12,288 exact | Below 1% error |
| TFC temperature and rainfall | 12,288 / 12,288 within 0.01 °C / 0.1 mm; both MAE 0 | Below 1% error |
| TFC forest category | 48 / 48 exact | Below 1% error |
| TFC terrain-noise height | 12,288 / 12,288 integer exact; MAE 0 blocks | Below 1% error |
| Surface material and fluid class | 11,693 / 12,257 exact (95.40%) | 4.60% mismatch |
| Sandstone variant | 266 / 266 exact | Zero mismatch in this sample |
| Surface fluid class | 12,231 / 12,257 exact (99.79%) | 0.21% mismatch |
| Near-surface voxel class | 76,282 / 76,770 exact (99.36%) | See rare-class precision below |
| Physical surface Y and class before decorations | 12,039 / 12,257 exact (98.22%) | 1.78% mismatch |
| Leaf-column projection | Precision 52.5%, recall 65.5%, F1 0.583 | Not within 1% |

Snow and ice are outside this accuracy pass. The material/fluid/surface denominators
exclude **31 observed frozen substrate columns**. The 840 visibly snow/ice-covered
columns are not all excluded: known natural substrate under thin snow remains
scorable. An incorrect snow prediction on non-frozen reference ground still fails.
The older material score used 12,288 columns, including those 31 frozen substrates.

The surface-material score requires exact solid block IDs, while water, lava and
magma use their fluid classes. The spread sample contains water and solid columns,
but no lava or magma positives; the existing volcanic references cover those
separately. The leaf score compares projected TFC tree leaves with every leaf block
in saved columns. Other mods also place leaves, so it cannot measure exact TFC tree
origins or species.

The terrain-height audit found a real bug before the latest correction: all 256
columns in the sampled dormant-shield chunk `(-113, 327)` were too high, by as much
as 26 blocks. Its volcanic age used glacial-shield constants. The source-backed
age profile correction now matches every saved terrain-noise height in this
sample. Fixing Java's `clampedMap` ordering then removed the dominant arid-sand to
snow material error. On the height-corrected version of the same sample, exact
ground matching improved from 86.21% to 91.13%. The retained first-run baseline
predates both fixes and records 85.34% material accuracy and 97.92% height
accuracy.

The badlands sampler now reproduces TFG's seeded sand/sandstone palettes,
shuffle order, biome blend weight, altitude/slope depth, dipping strata and
karst/mafic overrides. All 266 observed sandstone columns now match exact block
IDs. The normal map at local scales additionally uses 3D river-wall density,
noise caves, cave/canyon carvers and TFC aquifers to find openings and their floors.
These queries are cached and run in generation workers; distant regional tiles
retain the cheaper height-only pass.

The new independent vertical reference samples a 4×4 lattice through each of the
48 chunks, from Y=-32 up to min(noise height,128). Its 76,770 voxels are separate
from the 12,288 surface columns. **99.36% overall voxel classification does not
mean 99% accuracy for openings**: air precision is 91.24% and recall 100%; water
precision is 97.52% and recall 99.79%; lava precision is 90.48% and recall 95%.
Those totals compare the pre-decoration cave model with final saved blocks.
The later block-identity audit found 285 cave spikes, 36 calcite blocks and four
icicles among the 488 differing voxels. These are later features occupying
carved space, not proof of an incorrect initial cave shape. Another 127 are
hardened rock, consistent with the omitted erosion/support stage; their exact
origin is not proven individually. The earlier claim that the 472 solid-to-air
cases concentrated in three chunks was also incorrect: those three account for
135 of the 488 disagreements, and all their saved surface Y/classes match.
See [the scenario audit](SURFACE-MISMATCH-SCENARIOS.md) for exact blocks and hashes.

Tree root replacements now consume conditional weighted-state random draws and
retain replaced root states, so repeated root attempts see earlier writes. Forest
origins use the retained OCEAN_FLOOR_WG column. Minecraft's ProtoChunk updates
ChunkStatus.heightmapsAfter(); CARVERS/FEATURES use POST_FEATURES, which contains
the live heightmaps, not WORLD_SURFACE_WG/OCEAN_FLOOR_WG. Feeding new crowns into
the retained heightmap was tested and rejected: it reduced canopy F1 to 0.569.
The accepted physical-origin/root changes give F1 0.583, compared with 0.580 before
this pass. This is only a small improvement, not exact tree placement.

The latest scenario audit fixed **134 of the previous 707 surface failures**,
with **zero newly failing reference columns**. Forty-nine already-correct boulders
were hidden by the pre-decoration water mask; 52 steep river banks were missing
the explicit gravel builder; 33 volcanic-shore columns omitted their delegated
active-volcano land builder. River dispatch also needed the unrounded height
for the <=64 gate: storing only truncated Y incorrectly included 11 nearby banks.
The boundary correction preserves all 11 previously correct columns.

The remaining material disagreements include noncommuting soil patches from
neighboring source chunks. Replaying source-chunk orders reproduces the exact
saved state for 344 soil-only failures while the viewer's imposed global feature
order reproduces its wrong state. The saved fixture does not record scheduling
history, so substituting a guessed universal order cannot establish parity.
Missing clay discs, erosion/support, boulder projection and some shoreline
columns also remain. The [full scenario audit](SURFACE-MISMATCH-SCENARIOS.md)
records concrete coordinates, source conditions and unresolved cases.

The next follow-up adds the missing Rocky Plateau raw-rock branch and validates
two additional complete World 0 chunks: **512/512 exact surface blocks and Y**,
including a mixed soil/rock control. These chunks are outside the fixed 48-chunk
spread, whose score remains 95.33%. A separate volcanic grass patch remains
unresolved: at **-2767,4513** the save has grass/mollisol on hardened basalt while
the seed sampler selects a basalt cap. Installed source and bytecode agree on
the modeled cinder gate; changing its height threshold does not explain the
observations. See [the follow-up evidence](SURFACE-FOLLOWUP.md).

Biome, climate, forest category and terrain-noise height have zero measured
disagreement in these 48 chunks. This does **not** support a below-1%
claim for surface materials, openings or tree placement, and it cannot certify
unexplored coordinates. The tree forecast additionally depends on runtime feature
ordering and placement gates; its 2D leaf metric includes unrelated foliage. Those
source and sampling limits remain material barriers to a meaningful global 1%
error rate.

Reproduce the read-only capture and evaluation with:

```sh
cd native/terralens-rs
python3 tools/reference/extract_generator_accuracy.py /path/to/world0
cargo run --release --locked --features diagnostics --example accuracy_survey
```

The saved reference is [`world0-spread-48.json.gz`](../native/terralens-rs/tests/fixtures/generator-accuracy/world0-spread-48.json.gz).
The current per-chunk results, confusion pairs and source hashes are in
[`generator-accuracy-world0-scenarios-final.json`](data/generator-accuracy-world0-scenarios-final.json).
The previous 94.23% surface pass remains in
[`generator-accuracy-world0-surface-final.json`](data/generator-accuracy-world0-surface-final.json).
The separate voxel and physical-column results are in
[`caves-aquifers-world0.json`](data/caves-aquifers-world0.json).
The earlier height/color pass is retained in
[`generator-accuracy-world0-spread-final.json`](data/generator-accuracy-world0-spread-final.json).
The pre-correction comparison is retained in
[`generator-accuracy-world0-spread-baseline.json`](data/generator-accuracy-world0-spread-baseline.json).

The surface-profile fixture and original-Java noise control can be regenerated with:

```sh
python3 tools/reference/extract_surface_profiles.py /path/to/world0
python3 tools/reference/surface_noise_oracle.py
cargo run --release --locked --features diagnostics --example surface_profiles
```

See [surface caves and aquifers](SURFACE-CAVES.md) for modeled stages and limits.

The rock/spring follow-up corrects eight exposed diorite dike columns and one
spring-material column in the same immutable spread sample, with **zero newly
failing columns**. The result is 11,693/12,257 exact surface materials or fluid
classes (95.40%). This remains above the requested 1% error target. The separate
user-supplied spring site has 159/163 exposed quartzite matches, but still misses
156 water columns in the seed-only model; it must not be summarized as 97.5%
overall site accuracy. See [the investigation](HOT-SPRINGS.md) and
[the spread comparison](data/rock-spring-broad-accuracy.json).
