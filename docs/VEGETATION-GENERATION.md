# TFG vegetation generation notes

This records the installed TFG 0.9.25 / TFC 3.2.25 world-generation path for the Rust atlas. The local source snapshot under `/tmp/tfg-core-0.9.25` and the installed data pack are the evidence; these notes describe this pinned instance, not arbitrary TFC packs.

## Feature RNG: TFC override is authoritative

`net.dries007.tfc.world.TFCChunkGenerator.applyBiomeDecoration` overrides vanilla's `ChunkGenerator.applyBiomeDecoration` (`TFCChunkGenerator.java`, lines 303–371). Do **not** use vanilla `WorldgenRandom.setDecorationSeed(worldSeed, chunkX, chunkZ)` or its `base + featureIndex + 10000 * step` formula for vegetation.

For each decorated chunk TFC creates one `WorldgenRandom` wrapping `XoroshiroRandomSource(RandomSupport.generateUniqueSeed())`. It computes `baseSeed = Helpers.hash(128739412341L, originPos)`, where `originPos` is the chunk section origin (x/z are chunk minimum block coordinates; y is the world's minimum section origin). Before every placed feature TFC calls `Helpers.seedLargeFeatures(random, baseSeed, featureIndex, decorationIndex)`:

```text
random.setSeed(baseSeed)
seed = (featureIndex * random.nextLong() * 203704237)
     XOR (decorationIndex * random.nextLong() * 758031792)
     XOR baseSeed
random.setSeed(seed)
```

`Helpers.hash(salt, x, y, z)` uses Java signed-long wrapping arithmetic:

```text
h = salt XOR ((long)x * 501125321) XOR ((long)y * 1136930381) XOR z
h *= 0x27d4eb2d
return (int)h  // sign-extended when assigned to long baseSeed
```

The same `WorldgenRandom` object is reseeded and passed into `PlacedFeature.placeWithBiomeCheck`; all feature placement modifiers, feature attempts, and nested features consume its Xoroshiro stream in sequence. The random constructor's initial unique seed is overwritten by `setSeed(baseSeed)` for every feature. The TFC placement seed formula does not include the world seed; TFC's regional forest type/density inputs do.

The actual decoration step is `features[8]` for TFG `#tfg:in_biome/large_features/*` (the TFG forest is `tfg:earth/forest`, step 8, `FLUID_SPRINGS` ordinal in MC 1.20.1) and `features[9]` for TFG `#tfg:in_biome/surface_decoration/*` (wild crop patches, step 9, `VEGETAL_DECORATION`). The data-driven order is resolved globally with Minecraft `FeatureSorter`; the relevant `StepFeatureData.indexMapping` index, not a tag-local ordinal, is passed to the seed helper.

### FeatureSorter profile caveat

`native/terralens-rs/src/vegetation/assets/feature-order.json` contains a deterministic reconstruction profile using the 109 TFG Earth biome keys (`tfg:earth/<name>`) sorted by resource key. It records full per-stage order/index tables, a `featureBiomes` membership map for every feature, and every active crop feature. `tools/extract_feature_order.py` rebuilds this file from installed jars and the relevant KubeJS feature-tag script. For this profile, `tfg:earth/forest` is step 8 index 25; TFC wild crops are step 9 indices 43–65; active TFG crop patches are step 9 indices 66–79; palm fruit patches are step 8 indices 39–40.

Exact per-install runtime matching is not statically possible: `TFGBiomes.TFG_EXTENSIONS` is an `IdentityHashMap`, and `TFGBiomes.getAllKeys()` returns its key set. That possible-biome iteration feeds `FeatureSorter`'s first-seen global feature IDs; equal-ready DFS vertices are then selected by `(step, globalId)`. The installed TFG biome iteration can differ between JVM executions and shift some indexes. The extraction profile merges mod-jar tag resources by lexicographic jar filename and applies literal `event.add/remove` calls from `registerTFGOverworldPlacedFeatures` plus its palm-feature loop. Other dynamic KubeJS biome-tag additions are excluded: treating every script event as a raw ordered placed-feature list produced a cyclic graph, so the exact loaded tag semantics or live JVM order must be captured to match those changes. The JSON table is exact for its documented canonical profile, but should not be advertised as a guaranteed byte-for-byte match to a generated world unless the runtime feature-order profile is known.

## Forest feature behavior

The installed placed feature is `tfg:earth/forest` (`data/tfg/worldgen/placed_feature/earth/forest.json`), configured as `tfc:forest` (`data/tfg/worldgen/configured_feature/earth/forest.json`). It is included in TFG's `large_features` biome tags, which are at decoration index 8. Forest type is read from TFC's chunk-data forest layer at `(chunkX, chunkZ)`; forest weirdness and density are sampled once at block `(chunkMinX + 8, chunkMinZ + 8)`.

`ForestFeature.place` behavior from the installed TFC bytecode (`/tmp/ForestFeature.javap`):

1. Use the feature's one shared RNG; roll `per_chunk_chance` first and return when the draw exceeds it.
2. Sample tree and bush counts from their configured `IntProvider`s. Tree attempts are scaled by `(int)(requestedTrees * (0.6 + 0.9 * forestDensity))`.
3. For each tree attempt, consume `nextInt(16)` for x and z, query worldgen ocean-floor height, choose climate-valid configured tree entries using the same RNG, order/prune by climate mean and forest weirdness, then use the weighted continuation/species walk. Tree age/old-growth/dead variants and nested configured tree feature placements consume additional RNG.
4. Bush attempts follow the tree attempts. Groundcover and leaf piles use configured counts and further shared-stream attempts; a fallen tree can be tried after a successful tree.

The installed forest configs are in `data/tfg/worldgen/configured_feature/earth/forest.json`. Normal forest uses 5 tree attempts, 25 groundcover attempts, 2–4 leaf piles, 1–3 bushes, and allows old-growth spoiler variants. Edge uses 2 trees, 10 groundcover, 1–2 leaf piles, and 0–2 bushes. Sparse forest has a per-chunk chance of 0.08, 1–3 trees, 6 groundcover and 0–1 bushes. Old-growth forest uses 7 trees, 40 groundcover, allows old growth, 2–6 leaf piles, and 2–5 bushes. The `none` type has zero chance.

The Rust detail view resolves the installed configured tree features and projects their authored NBT geometry, including random, stacked, trunkless stacked, overlay, and krummholz variants. It consumes template, mirror, rotation, trunk, palette, and root-placement draws from the shared feature RNG. White cedar overlays follow Minecraft's full/other/block-entity grouping and Y/X/Z block order, applying one integrity draw per processed block before choosing the highest accepted block in each column. Installed KubeJS configurations take precedence; for example, this pack's large sequoia is trunkless and has no root system.

Forest-entry order follows the active merged tags, including AFC's `replace:true` override of `tfc:forest_trees`, then TFG's appended AFC normal-tree tag. This resolves 61 ordered entries. `native/terralens-rs/tools/extract_tree_templates.py --check` independently rebuilds and compares the embedded metadata from the installed jars and KubeJS files, using temporary extraction directories. It needs Python only for development; the application embeds the resulting data and runs entirely in Rust.

The result is a predicted top surface, rather than a full three-dimensional chunk. Terrain-dependent root replacement and tree survival remain approximate. Bushes, leaf piles, groundcover, and fallen trees are not emitted. Saved chunk blocks take priority when available.

## Forest type, weirdness, density

In `RegionChunkDataGenerator.create(worldSeed, ...)` (`/tmp/tfg-core-0.9.25/src/main/java/net/dries007/tfc/world/chunkdata/RegionChunkDataGenerator.java`):

1. Initialize `XoroshiroRandomSource(worldSeed)` then call `setSeed(worldSeed XOR nextLong())`.
2. Consume seeds in this order: rock area (`nextLong`), layer-height noise (`nextInt`), skew-X (`nextInt`), skew-Z (`nextInt`), forest type layer (`nextLong`), forest weirdness (`nextInt`), forest density (`nextInt`). The forest type layer is world-seed-dependent.
3. Weirdness is `OpenSimplex2D(seed).octaves(4).spread(0.0025f).map(v => 1.1f * abs(v)).clamped(0, 1)`.
4. Density is `OpenSimplex2D(seed).octaves(4).spread(0.0025f).scaled(-0.2f, 1.2f).clamped(0, 1)`.
5. Sample both at `(chunkMinBlockX + 8, chunkMinBlockZ + 8)`. Forest type is `forestTypeLayer.get(chunkMinBlockX >> 4, chunkMinBlockZ >> 4)`.

The Rust implementation ports TFC's `OpenSimplex2D` wrapper and seed-consumption sequence, rather than substituting vanilla `NormalNoise`. Java reference samples using the installed classes independently verify the density and weirdness values. The existing region/layer implementation supplies forest types.

TFG's installed `RegionChunkDataGeneratorMixin` only adjusts river rainfall in this path; it does not replace TFC forest type, weirdness, or density generation.

## Wild crops and expected exactness

The TFC wild crop placed features live in `data/tfc/worldgen/placed_feature/plant/wild_crop/`; installed TFG Earth crop additions are in `data/tfg/worldgen/placed_feature/earth/crop/`. TFG adds 14 crop patches to TFC's `feature/crops` tag from `kubejs/server_scripts/tfg/overworld/tags.overworld.js`. The active biome-surface patches comprise 23 TFC crops and 14 TFG crops at step 9; TFG's two palm-fruit patch features are separately inserted into coastal large-feature biome tags at step 8. Two additional palm placed features (`coconut` and `date`) are child features referenced by those patches, not biome-list entries, so they have no `FeatureSorter` index of their own. TFC examples such as `wheat_patch` apply a rarity filter (chance 80), in-square offset, and TFC climate/forest predicate; their configured patch uses `minecraft:random_patch` (six tries, xz spread five, y spread one) with a simple-block crop state. Their surface/replaceable/survival predicates affect whether an attempt becomes a plant. The full per-feature definitions and block names are included in the generated `native/terralens-rs/src/vegetation/assets/crop_features.json`.

For a performant preview, generate attempt positions with the exact profile index and deterministic chunk RNG, then filter against terrain, biome climate, forest and plant survival rules. Keep these as **predictions**, not discovered/generated structures. Do not derive actual crop locations from crop frequency alone. A display should distinguish saved blocks from predictions; if profile index is unknown, either show a visibly approximate density layer or omit exact point markers rather than imply certainty.

The Rust detail generator implements those deterministic attempts for all 37 simple crop patches and both palm patches in the profile. It preserves nested patch offset draws, spreading-crop direction draws, climate/forest gates, neighboring-biome eligibility, and KubeJS coconut/date template selection. Palm templates enter only the tree path and retain their authored canopy heights. Neighboring source chunks contribute plants across chunk boundaries, and the topmost canopy or crop owns each displayed surface column. Height, saltwater, replaceability, and plant-survival checks use the preview terrain model; they can differ from Minecraft's final blocks. Hover cards identify seed predictions separately from exact saved blocks.

## Installed evidence inspected

- TFG Core Modern 0.9.25 sources extracted to `/tmp/tfg-core-0.9.25`.
- TFC source: `net/dries007/tfc/world/TFCChunkGenerator.java`, `net/dries007/tfc/util/Helpers.java`, `net/dries007/tfc/world/chunkdata/RegionChunkDataGenerator.java`.
- TFC bytecode snapshots: `/tmp/ForestFeature.javap`, `/tmp/RegionChunkData.javap`; Minecraft sorter snapshot: `/tmp/FeatureSorter.javap`.
- TFG biome ordering source: `net/dries007/tfc/world/biome/TFCBiomes.java` and installed mixin `su/terrafirmagreg/core/mixins/common/tfc/new_ow_wg/TFCBiomesMixin.java` (maps extension keys to possible biomes).

No game process was started and no save data was modified for this research.
