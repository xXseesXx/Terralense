# Ground materials

Enable **Layers → Ground blocks** (shortcut `9`) to show soil, sand variants,
gravel, exposed rock, duff, mud, snow and ice. Saved farmland, paths and raw
sandstone also keep their own material colors. It composes with relief, contours,
trees and the other map layers. Grass coloring also uses this material selection:
grass receives the TFC climate tint, while other materials keep their own colors.
Water, lava and magma retain their dedicated map colors.

The ground hover row shows the block and Y. Its hover text identifies saved blocks
or the TFG surface-rule estimate. **Map → Display options → Tooltip → Ground
material** makes that row available on other layers, too. Detail terrain uses the
same material palette instead of assigning a generic grass/sand/snow category to
an entire biome.

At local scales, seed terrain also includes TFC's raw, cobble and mossy boulders,
their small-boulder patches, and TFG's duff, alfisol and podzol discs. Boulder
silhouettes retain their block heights in material-map relief and contours. These
features appear through the existing Ground blocks and Grass colors options;
they do not require enabling detail terrain. Saved chunks supply their actual
surface blocks whenever available.

For seed 0, one close-view boulder example is at `-65538, -65533`; special soil
examples include duff at `-31232, -32768`, alfisol at `-21504, -24064`, and podzol
at `0, -18944`. These are seed-sampler witnesses, not independently generated
Minecraft reference chunks.

## Saved terrain

At local scales (up to 4 blocks per pixel), opened worlds supply the highest
natural terrain material beneath leaves, plants, crops, loose rocks and thin snow.
Fluid inside a waterlogged plant or loose block keeps the saved water plane. The
reader excludes common building blocks and containers; a natural terrain block
used in a building cannot always be distinguished from terrain.

The source cache includes the save path and refresh revision. Chunk data stays in
the isolated engine, uses bounded caches and never modifies the save. At distant
scales the renderer uses the seed sampler without scanning the visible save area.

In the release child-engine check, a medium saved-world ground tile took 1.67 s
after this scale limit, down from 13.14 s with broad chunk reads. A local saved
ground tile took 0.67 s, and small local ground PNG exports took 0.11–0.16 s.
These are engine timings, excluding GPU presentation; cached panning reuses tiles.
See [`data/ground-render-performance.json`](data/ground-render-performance.json).

With surface decorations enabled, a cold local seed tile took 0.27 s, or 0.47 s
with relief and contours. A tile sampling at four blocks per pixel took 2.20 s.
The distant saved and seed views produced identical tile payloads, confirming
that they still avoid save scans. These are isolated engine checks with two
worker threads, rather than UI frame-rate measurements. See
[`data/surface-decoration-performance.json`](data/surface-decoration-performance.json).

## Seed-only terrain

`ground.rs` and `ground/` reference TFG Core 0.9.25, source commit
`f4929e738e83dbc8d46680276faa67520dffa172`, with TFC 3.2.25. They include the soil
climate bands, position-hash and patch transitions, warm oxisol and volcanic
mollisol profiles, local rock sand types, shore sand noise and gravel selection,
lowlands, flats, dunes, lava-flow materials, and centered volcanic surface rules.
The temperature used for climate bands includes elevation cooling; the warm-soil
transition uses mean annual temperature, as in the source.

The material sampler selects the surface builder from the game's weighted
normal/shore/ocean biome partitions and river gate. Its normal and shield-volcano
builders use TFG's cached quart slope map and altitude-dependent soil depth, so
steep terrain can expose the appropriate gravel or raw rock. Rainfall below the
surface table's minimum clamps to its arid band, rather than entering the polar
snow bands. Below sea level, the ordinary solid cap includes the extra cell
retained by the filler's upper density slide.

This is a surface estimate rather than a complete Minecraft density and
decoration pass. Special carving, glacial builders, forest
canopy effects, ash and crater-edge replacements can still differ in unexplored
terrain. Saved ground overrides these estimates. Do not interpret saved-reader
accuracy as seed-only generation accuracy.

The wider deterministic 48-chunk World 0 audit measures 91.13% exact solid-block
or fluid-class matches across 12,288 columns; it is separate from saved-reader
equality. Its remaining errors and source-stage dependencies are recorded in
[the generator scorecard](GENERATOR-ACCURACY.md).

The decoration sampler follows the installed stage-8 feature order and biome
gates, rarity and climate filters, per-chunk RNG, metaball boulder shapes, small
boulder shapes, and exact soil replacement block lists. It preserves the pack's
old-growth duff feature link to the normal-duff configuration. It projects the
surface rather than generating a full Minecraft chunk: placement height uses the
estimated terrain before earlier tree canopies, and overlapping features across
source chunks can differ from Minecraft's decoration order. Fine terrain and
these height-dependent checks remain approximate in unexplored areas.

Source features and their clipped chunk surfaces use bounded caches. Tile
rendering only enumerates these decorations at four blocks per pixel or closer;
distant ground views keep the lightweight surface-builder sampler. Hover and
detail terrain share the same close-scale material selection. No save is written.

## Intrusive rock patches

Local material views include TFC's granite, diorite and gabbro dikes, which replace
host rock independently of the regional rock-layer sampler. They follow the
installed pipe-vein geometry, source-chunk traversal, rarity, configuration-name
seed, height limits and target-chunk random stream. Raw/gravel hosts become raw
dike rock; hardened hosts stay hardened. The installed configurations stop at
Y180, even though the pipe geometry's bounding box can extend higher.

The replay consumes density draws throughout the required clipped traversal and
provider draws only for replaceable blocks. Core’s basal hornfels/bedrock band
is excluded using its original four-layer noise pass, avoiding provider draws
for non-host blocks. An independent 512-column saved control validates these
exclusions over Y=-64..-51. Compact eligibility grids avoid allocating strings
for underground rock states; a shared bounded cache retains only exposed results across engine workers. Replay
stops after the feature's last possible surface write; every preceding vein and
provider draw is preserved. Distant
views retain the lightweight terrain sampler. Eight independently saved diorite
columns that previously appeared as chert gravel are now corrected.

This does not yet replay all preceding ore, erosion, clay and kaolin replacements.
Those can affect provider RNG consumption and individual dike boundary cells.
Saved terrain still takes precedence at local scales. See
[the spring-site investigation](HOT-SPRINGS.md) for a different kind of rock
patch at the user's reference location.

## Validation and palette

The independent Python Anvil decoder captures all 256 columns of eight fixed
World 0 chunks: **2,048 columns**, including **836 covered columns**. Offline Rust
tests decode stripped copies of the original block-state sections and match every
selected block and Y. A second test writes a temporary Anvil region and verifies
both ground and grass map colors beneath actual cover. Neither requires Minecraft
or the original save. Waterlogged cover and mixed-layer raster/export agreement
have separate checks.

A layer-switch regression test also verifies that saved material overrides cannot
contaminate the water masks shared by other layers. Independent Java RNG traces
check both boulder forms, material choices, and soil origin/radius sequences,
including the draws consumed inside each soil disc. The normal build keeps
diagnostic tools disabled. These regressions run in the complete release suite.

The bounded seed-only comparison and its limitations are recorded in
[`data/ground-materials-world0.json`](data/ground-materials-world0.json). Its fixed
sites emphasize difficult volcanic and forested terrain and are not a random
sample of the world. Repeat it with:

```sh
cd native/terralens-rs
cargo run --release --locked --features diagnostics --example ground_accuracy
```

To refresh the reference captures from a read-only save:

```sh
python3 tools/reference/extract_ground.py /path/to/world
```

The bundled ground palette contains averaged RGB values from the installed
textures; it does not include the full texture images. See
[`assets/ground-colors/README.md`](../native/terralens-rs/assets/ground-colors/README.md)
for extraction details.
