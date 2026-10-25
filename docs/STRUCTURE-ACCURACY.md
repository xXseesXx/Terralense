# Seed-zero structure accuracy

The current Overworld scanner was compared with the user's expanded world `0` using exact
structure registry ID and start-chunk coordinates. Predictions were generated from the seed and
world settings before consulting saved starts. The viewer's saved-chunk suppression was not
used to improve the scores.
The complete [JSON report](data/structure-accuracy-seed0-expanded.json) records every mismatch.

The completed comparison covers **185,802 fully generated chunks** across **358 regions**.
The saved scan decoded 315,566 records in 933 Anvil region files. Chunks with missing, incomplete,
malformed or mismatched structure metadata were excluded from the comparison. There were no
decode errors, truncated prediction passes, or save changes during the audit.

| Result | Count | Denominator | Error rate |
| --- | ---: | ---: | ---: |
| Correct structure starts | 450 | — | — |
| Extra predictions | 1 | 451 predictions | 0.22% |
| Missed structures | 1 | 451 saved starts | 0.22% |
| Combined disagreement | 2 | 452 distinct predicted or saved starts | **0.44%** |

Combined disagreement is `(false positives + false negatives) / (true positives + false
positives + false negatives)`. Empty chunks are not counted as true negatives to inflate
accuracy. All three aggregate error measures are below the requested 5% on this observed area.
This is structure-start accuracy for this seed and saved area, not a guarantee about every
structure type, unexplored seed, individual room, loot roll, or block-level layout.

All **20 saved villages** in the completed cohort matched: four Arabic, six Malay, five
living Norse, four ruined Norse and one Yurts. There were no extra village predictions.

Both remaining mismatches are ocean Moai: 94 correct, one extra and one missed. Its per-type
false-positive and miss rates are each 1.05%. Mineshafts have 13 correct starts and no errors
in this cohort. Every supported type has at least one saved start in the comparison; this
does not establish low error for types represented by only a few examples.

## Fixes behind the result

The scanner previously omitted current Lithostitched spawn conditions when loading structure
definitions. The extracted profile now includes all five applicable modifiers and their resolved
biome grids and height ranges. Grid checks use the integer bounding-box center of the selected,
rotated jigsaw starting piece, as Minecraft does, rather than the chunk origin or marker center.
The starting mineshaft's delegated pool element also supplies its actual stub geometry. TFG's
`getBaseHeight` returns terrain height for these checks even for `WORLD_SURFACE_WG`; submerged
ground is not raised to sea level when evaluating a minimum-height condition.

The current height model also applies the original volcanic, shoreline and tide stages and
the registered biome blend groups before river carving. These corrections affect structure
height conditions as well as the map. The audit was rerun after those changes on October 24,
2026; fingerprints include the height, shore, river and blend metadata. Independent volcanic
height evidence and its scope are described in [volcanic terrain](VOLCANOES.md).

The reported Malay village near **15864, 15850** is rejected by its surrounding-biome exclusion
grid. Its selected stub is **15851, 15845**, and the exclusion grid includes forbidden ocean
biomes. The saved chunk `(991, 990)` independently has no structure starts. Malay villages remain
eligible in wet lowlands and salt marshes; flooding alone is not a global village veto.

When a saved world is open, the map and search additionally respect intact structure metadata
from fully generated chunks. They suppress unconfirmed placement candidates in completed chunks
and retain actual saved starts. Unknown and incomplete chunks do not veto candidates. This
separate display behavior is never fed into the accuracy calculation.

## Reproduce

The diagnostic executable reads the supplied world and writes its JSON report to stdout:

```sh
cargo run --release --locked --features diagnostics \
  --manifest-path native/terralens-rs/Cargo.toml --example structure_accuracy \
  -- /path/to/world > structure-accuracy.json
```

The report includes per-type and per-family denominators, complete mismatch lists, input
fingerprints and before/after save stamps. Let Minecraft finish saving before measuring. A
changed input, decode error or truncated comparison invalidates the acceptance result. The
audit never writes to a save. Restore the normal app with `./build.sh` after diagnostic builds.
