# Hot springs

Hot springs are natural terrain in the seed sampler. They have no structure
markers and do not appear in the structure sidebar. Spring water participates in
the same visible-water mask as other surface water, so it stays blue on biome,
climate, geology, height and forest maps. Ground blocks and Grass colors expose
the gravel floors and hardened-rock rims. Local terrain shading and contours
also use dry basin depths without requiring the Ground layer.

For seed 0, the sampler produces an active spring at `14386, 13643` and a dry
basin floor at `-319245, 16110`. These are procedural witnesses; they are not
claimed as independently matched Minecraft spring placements. Opened chunks
continue to supply actual terrain and tree surfaces at close zoom.

## Placement and geometry

The model follows installed TFG Core 0.9.25, TFC 3.2.25 and GTCEu 7.5.3:

- `tfg:earth/fluid_vein/spring_water` is at decoration step 3, index 2.
  Its rarity is 1/30, followed by in-square placement and the spring-water
  bedrock-fluid vein predicate. Its configured selector has 12 wet entries and
  four dry entries. The separate TFC placed-feature rarity of 1/50 is not nested.
- Bedrock-fluid selection uses the eight-chunk vein coordinate, packed signed
  coordinates, world seed, MC Xoroshiro, Java weighted selection, biome and
  climate weights, and the installed registry order. It includes the registration
  of the dimension-filtered Moon/Mars/Venus fluids when reconstructing that order.
- `tfc:random_empty_hot_spring` uses its source rarity and six-biome membership;
  its four selector entries all carve dry basins. `tfc:lava_hot_spring` retains
  its underwater placement and magma/hardened-rock contact behavior.
- The source Metaballs2D footprint uses a forked RNG, source float precision,
  transformed weight sums, terrain edge detection and per-column depth draws.
  Rock materials come from the origin's rock at Y=0. Overlapping features use
  decoration step and index order.
- Dry basins preserve TFC's `filledEmptyPositions` rule: earlier carved air
  cells inside a basin do not count as open edges for later cells. Without
  that position check, later interior columns are incorrectly classified as
  edges, producing false rim blocks and consuming extra rare-hole draws. The
  projection now carries the source's filled air positions at their exact Y.

The sampler uses bounded source and target caches, stores empty targets without
allocating a block raster, and rejects rare candidates before terrain work. The
active wrapper is registered in all 109 Earth biomes, so its more selective vein
predicate replaces a redundant neighboring-biome union scan.

## Verification and limits

Independent Java reference programs verify the vein selection, negative chunk
coordinates, active-feature origin and selector, metaball parameters, complete
29-by-29 footprint, and RNG continuation. Rust integration checks spring water
on every layer and dry-basin raster/export relief agreement. The reference
sources and JSON oracle are under `src/hot_springs/` in the Rust crate.
See [validation](data/hot-springs-validation.json)
and [cold engine timings](data/hot-springs-performance.json).

This is a column projection over estimated terrain. It does not generate a
Minecraft chunk or simulate all prior features or fluid ticks. Neighboring source chunks use a
stable preview order. The feature order is reconstructed from installed resources and calibrated against
saved World 0 spring geometry; live JVM possible-biome order remains a limitation.

## World 0 reference at -319262,16107

The supplied point is grass/loam at Y87. The nearby contrasting rock patch is a
spring basin centered near `-319250,16107`: its hardened/gravel rock is quartzite
from the origin's Y0 layer, whereas the surrounding surface host is chert. It is
separate from the granite/diorite/gabbro dikes.

An independent, read-only capture retains all 2,304 columns in the surrounding
nine FULL chunks, including 163 exposed quartzite columns and 156 water columns.
The calibrated feature order puts the empty spring at stage 8 index 30; the old
resource-key order placed it at 31 and missed the entire basin. Only the two
parallel spring indices change; forest, crop and all other stage orders remain
unchanged. See [the order profile](VEGETATION-GENERATION.md#featuresorter-profile-caveat).

The basin model now matches **159/163 exposed rock materials** and **2,144/2,304
complete surface block/elevation pairs**, up from 1,984 surface-material matches
before this correction. These are seed-only results. The major remaining local
error is the missing water in the seed-only model: **0/156** observed water
columns, with no extra water columns. Four exposed rock columns also differ.
The opened-world path separately preserves all 156 observed water columns,
including water beneath plant cover, without flooding the saved dry rim.

The dry-spring pass tracks interior carved-air positions separately from rare
rim holes and retains underlying soil at holes. The final underground fissure
pass and later lake filling are not included in this surface projection. A
source-based prototype added substantial regional generation time without
improving the independently measured exposed-rock result, so it is not shipped.

At this reference, one modeled rim hole would let the later flood fill escape
east and abort, while the saved chunk has a hardened quartzite wall there. A standalone
Java transcription over the independently captured noise-height inputs reproduces
the same 159/163 basin-rock result. This rules out a simple Rust conversion error
in that basin loop; it does not identify which earlier mutable-world state
caused the discrepancy. Prior feature heights, non-air vegetation, ore-dependent
fissure RNG and chunk decoration order still need a live pre-feature capture to
separate. Do not interpret this as exact spring-water prediction.

Reproduce the source control and seed-only score without a game process:

```sh
cd native/terralens-rs
python tools/reference/dry_spring_oracle.py --output /tmp/dry-spring-java.json
cargo run --release --locked --features diagnostics --example dike_site_accuracy
```

The original fixture filename `ground/dike-user-site-seed0.json` is retained for
continuity; its provenance describes the actual user site and read-only region
fingerprint. Reports are under `docs/data/rock-spring-*`.
