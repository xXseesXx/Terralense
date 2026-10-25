# Surface litter

Surface litter is an optional Overworld layer available in Layers and Map → Terrain
detail. Fast leaves it off. Full enables it alongside terrain and tree detail.
Turning it off restores the underlying terrain; no clutter is baked into the ground
material reader or its caches. The layer is sampled only at four blocks per pixel
or closer, and does not paint open water. Tree crowns stay above it.

## Source placement

The pinned source is TFG Core 0.9.25 / TFC 3.2.25 with the installed instance's
feature order and forest entries. Runtime code is Rust. Bounded source-candidate
and target-chunk caches preserve absolute coordinates; the target combines nearby
source chunks in stable feature/source order and clips their output. Canceled
partial results are not cached. World, settings and saved-source revision identify
cache entries; season changes reuse physical placement.

- Forest twigs and fallen-leaf piles continue the existing forest RNG after tree
  attempts and bushes. Ground debris requires a successful tree. Leaf clusters
  retain the source origin's Y rather than following every neighboring slope.
  The existing tree-only output is unchanged when litter is enabled.
- Forest and shoreline sticks, forest salt licks, salt-flat salt licks and TFG dry
  grass follow their installed rarity, in-square, climate, biome, random-patch and
  nested heightmap rules. Dry grass uses ten attempts and an XZ spread of fifteen,
  its dry-plantable support rule, elevation-adjusted climate and high-tide exclusion.
- Surface loose rocks use the stage-10 random patch: eight attempts over spread
  seven, with `tfc:loose_rock` resetting each candidate to `OCEAN_FLOOR_WG`.
  Although the outer feature starts at the world-bottom decoration origin, its
  nested heightmap is essential; it is not a bottom-only placement. Local rock,
  rainfall above 250, source boolean/count draws, fluid and ice/support gates
  select ordinary or mossy rocks.

Placement remains a surface projection. It does not reconstruct every earlier
plant, container, structure, falling-log mutation, full live heightmap or the JVM's
complete feature order. Such differences can change survival and RNG continuation.
Saved blocks override the projection at generated chunks, including explicitly empty
saved clutter columns. This is not a claim of exact final-chunk litter parity.

## Appearance

The compact palette is extracted from the installed texture sheets with Xaero's
square sampling grid, alpha cutoff and equal RGB averaging. Texture coverage is
alpha-composited over the substrate. Loose rocks use their raw-rock texture; mossy
rocks use mossy cobble. Species twigs use their log-end texture. Sticks, dry grass
and salt are not given an unrelated grass tint. Deciduous fallen leaves use their
seasonal species tint, while conifer litter keeps TFC's fixed orange tint. Living
leaf blossom handlers are not applied to fallen leaves.

This follows the installed Xaero texture/tint operations; exact Minecraft model
variants, block lighting and geometry-dependent color choices can still differ.
No full texture images are copied into the application for these palettes.

## Validation

The production saved reader is tested against independently decoded World 0 NBT
columns with real loose/mossy rocks, eucalyptus twigs/leaves, pumice, sticks and
other groundcover. Tests separately cover optional rendering, no water/coarse-scale
overlay, negative-coordinate clipping, texture profiles, tint rules, canceled
publication and unchanged tree output. Direct ZIP-extracted feature JSON controls
check placement constants, including the nested loose-rock heightmap.

Maintenance commands:

```sh
cd native/terralens-rs
python tools/reference/extract_litter_rules.py /path/to/TFG-Core.jar --output /tmp/litter-rules.json
python tools/reference/extract_forest_litter.py
cargo run --release --locked --features diagnostics --example litter_accuracy
```

The saved-top comparison records observations separately from seed projection;
covered ground cannot be evaluated from the highest saved block alone.

The existing eight-chunk saved-top fixture has 62 exposed recognized debris
columns. The seed projection matches 22 exact block/Y observations, including
both nearshore rocks, all eleven loose/mossy rocks in the cinder control, four
rock columns at the raw-rock tuya and three rocks at the river-bank control.
It projects 66 clutter columns overall; 20 lie below a higher saved top block,
so top-down observations cannot directly score their ground occupancy. Pumice,
feathers and seaweed are read from saves but their seed features are not modeled.
These denominators are deliberately separate from production saved-reader tests.
The report is [the seed-only comparison](data/surface-litter-world0.json).

Six normal-engine PNG exports exercise Fast regional view, seed litter off/on,
saved cinder debris, saved forest canopies and seed forest canopies. The seed
cinder output changes when litter is enabled; exports use the same overlay and
sampling plan as the live map. See [export checks](data/terrain-profile-exports.json).
