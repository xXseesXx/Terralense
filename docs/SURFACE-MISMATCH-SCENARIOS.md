# Surface mismatch scenarios

This audit starts from the earlier **707 mismatches in 12,257 non-frozen surface
columns** from World 0. It records every failed column, base material, physical
floor/water, selected and original builder biomes, slope, early rock decoration,
late soil state, and all touching boulder/disc source chunks. The World sampler
has no save source. Reference blocks are supplied only to the independent scorer.
Snow and ice retain the earlier exclusions.

The scenarios below distinguish source-rule defects, experimentally reproduced
order dependence, and observations whose exact generating history is unavailable.
Counts describe the pre-fix snapshot. The fixes resolve **134/707 failures**
with **zero new failing reference columns**: exact material/fluid matching rises
from **94.23% to 95.33%**, and water-class matching from **99.39% to 99.79%**.
All 12,257 columns are rescored; the remaining surface failure count is **573**.

| Scenario | Observed columns | Concrete example and evidence |
| --- | ---: | --- |
| Soil patches from different source chunks do not commute | 344 | At **-101810, -14333**, local duff followed by a neighboring podzol disc produces duff/podzol; running the podzol source chunk first leaves grass/podzol, exactly as saved. |
| Water mask hides an already-correct emergent boulder | 49 | At **8856, 25749**, the aquifer ends at Y=62 and the modeled chalk boulder is correctly at Y=63. The final water test ignored that later rock. Fixed. |
| Steep river banks bypass their river surface builder | 52 | At **-130031, -22048**, original dune-sea biome and slope 6.3 select basalt gravel; at **15134, 14793**, original rolling hills and slope 10.4 select rhyolite gravel. The viewer instead chose generic sand/grass. Fixed for the source's explicit gravel branch. |
| Volcanic shore omits its delegated land builder | 34 rows in the old-shield-shore sample | At **-99478, -21584**, TFG's old shield shore delegates to the active volcanic builder. Flow 0.88619 and material noise -0.05738 select raw basalt at Y=68, not generic soil. The land dispatch fixes 33 of these 34 observations; the remaining sand/gravel case is unresolved. |
| Earlier clay patches change later soil eligibility | 33 clay-grass observations | At **-112507, -15552**, the source's step-2 radius-3 clay disc is centered at **-112505, -15552**, placing clay grass inside its height range. Later duff cannot replace clay grass. This system is missing. The separate 18 oxisol clay-grass observations have not been traced to a canonical disc origin. |
| Boulder projection differs in shape, material or placement | 67 early-decoration observations | At **-103500, -14134**, the viewer emits mossy claystone at Y=78; saved ground is duff/podzol at Y=76. Other cases swap raw/cobble/mossy variants. The modeled candidate is identified, but this audit does not isolate its exact RNG/placement-history cause. |
| Surface geometry or later removal changes a shoreline | 13 saltwater/sand observations | At **-101816, -14325**, saved saltwater is at Y=62, cave air at Y=63–67, and normal air from Y=68. Raw height metadata and the model retain sand at Y=68. The precise shore-density/erosion cause is unresolved. |
| A raised natural rock surface has no modeled candidate | 30 shale columns above the modeled cap | At **-258735, 8368**, saved raw shale is at Y=69–70 over raw height 68. The model has no boulder candidate. The observation does not by itself prove which placed feature produced it. |

These rows are **overlapping diagnostic scenarios**, not a partition to add
against 707. In particular, clay or volcanic base errors may be changed again by
later soil discs. The complete per-column report preserves those stages.

Builder dispatch also exposed a fractional-height boundary: TFG checks the
**unrounded** height against 64 before choosing the river builder. At
**15135, 14792** and **-130032, -22048**, the stored height is 64 but the true
height exceeds 64. The viewer had discarded the fraction. Retaining it for
selection preserves their grass/sand banks; all 11 such boundary controls remain
correct after the gravel fix.

Biome-only rendering now marks covered mountain rivers in dark blue. This is a
visual annotation: their solid roof is not changed to water, and other data
layers retain their normal values. Ground/grass layers can still display the
surface material when enabled. Tests cover ordinary and stacked biome modes,
colorblind colors, explicit grass coloring and a low dry bank control.

## The soil-order proof

TFC `TFCChunkGenerator.applyBiomeDecoration` sorts features **within each source
chunk**. Its soil discs can write into adjacent chunks. The viewer instead sorts
all nearby discs by feature index globally. For state-dependent replacements,
those operations have different outcomes.

The retained simulator enumerates every permutation of whole source-chunk groups,
while preserving feature order inside each chunk. For **344 rows**, its global
ordering reproduces the viewer exactly, the base and pre-forest material agree,
and an alternative source-consistent chunk order reproduces the saved block
exactly. The three largest classes account for 317 of these:

- 175 grass/podzol → duff/podzol;
- 116 grass/alfisol → duff/alfisol;
- 26 grass/podzol → duff/alfisol.

For the first example, the podzol disc comes from source chunk **-6363, -897**
(center **-101802, 76, -14337**, radius 10); the duff disc comes from **-6363,
-896** (center **-101808, 72, -14331**, radius 3). Podzol grass is absent from the
duff replacement table, so applying podzol first protects that final state.

This proves the noncommuting rule and reproduces the observed outcome; it does
**not** recover each saved chunk's actual scheduling history. A seed alone does
not encode a player's chunk-loading sequence. Replacing the imposed global sort
with one guessed order would shift errors elsewhere. Saved final blocks remain
authoritative where available.

## The cave-voxel score was measuring later features too

The 488 vertical-voxel disagreements are separate from the 707 surface-material
failures. Reading their exact blocks from the save, with **all region SHA-256
fingerprints matching** and **488/488 classes matching the immutable fixture**,
found:

| Saved block at a mismatched voxel | Count |
| --- | ---: |
| Rock spike | 285 |
| Hardened rock | 127 |
| Calcite | 36 |
| Icicle | 4 |
| Other blocks/fluids | 36 |

TFG runs cave spikes, columns, calcite and icicles after carving. Consequently,
325 of these saved solids are later cave decoration occupying carved space;
classifying them as initial cave-shape failures was incorrect. The missing
`ErosionFeature` can support collapsible columns and harden rock above fragile
blocks, and is relevant to the 127 hardened-rock observations. The audit does
not prove that every hardened voxel was written by that feature.

Shore density is another actual missing behavior: eight `ShoreNoise` samplers
override `noise(y, initialNoise)`. The current density pass approximates the shore
sum as the initial biome noise. For example, sandy shores return zero above
weight 0.5, and a height-dependent carving value near weaker edges. This is a
source gap, but the 13 saved saltwater columns have not been proven to follow a
specific missing override; erosion and later removal must also be considered.

## Reproduce and inspect

From `native/terralens-rs`:

```sh
cargo run --release --locked --features diagnostics --example surface_mismatch_audit
cargo run --release --locked --features diagnostics --example surface_mismatch_audit -- -101810 -14333
python3 tools/reference/audit_soil_order.py
python3 tools/reference/decode_voxel_mismatches.py /path/to/world0
```

Evidence is retained in the compressed [707-column before snapshot](data/surface-mismatch-world0-before.json.gz),
[soil-order proof](data/soil-order-world0-proof.json.gz), and
[exact cave-voxel identities](data/surface-voxel-block-identities.json.gz).
The [after snapshot](data/surface-mismatch-world0-after.json.gz) and
[current accuracy report](GENERATOR-ACCURACY.md) record the post-fix score.
No Minecraft process was launched or save file modified during this audit.
