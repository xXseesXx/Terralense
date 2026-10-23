# Saved planetary biome reference

`seed0_biomes.json` is a compact, immutable reference extracted from the user's existing seed-0 TFG save in Prism. It contains no chunk payload, player identity, UUID, or account data. The source save was opened read-only; Minecraft was not launched and the save was not modified.

`seed0_ores.json` adds 256 exact saved ore block observations in each planet. Each record stores the namespaced block ID, absolute block coordinates, and containing chunk coordinates. Entries were read from generated Anvil block-state palettes and are distributed across region files. This bounded sample is useful for checking ore feature material and placement vicinity; it is not an exhaustive resource census or an oracle for every predicted vein.

The extraction reads generated Anvil chunks from `dimensions/ad_astra/moon/region` and `dimensions/ad_astra/mars/region`. `seed` was confirmed as `0` from `level.dat` (`Data.WorldGenSettings.seed`). For each dimension, 4,096 deterministic, spatially distributed sample columns were selected from saved chunk data. Each biome sample records its absolute block `x/z`, the saved surface Y, and the biome palette entry from the corresponding saved 4×4×4 section biome data at that column and height. Matching surface heights come from the saved `WORLD_SURFACE` heightmap (decoded as the 1.20-era padded 9-bit long array, then adjusted using the save's dimension minimum Y). Saved Moon sections start at Y=0. Saved Mars chunks use the dimension type minimum of Y=−64, while the active Mars noise settings span Y=−32..351. For all 64 sampled audit columns checked against decoded block-state arrays, the saved `WORLD_SURFACE` value matched the highest non-air block using `stored value - 1 + min_y`.

Extraction summary:

- `ad_astra:moon`: 4,096 sample columns; all 10 observed biome IDs are represented; sampled X 338–1,790, Z −190–2,478, surface Y 5–132. The saved generated coverage found for this dimension has no negative-X chunks.
- `ad_astra:mars`: 4,096 sample columns; all 8 observed biome IDs are represented; sampled X −622–1,326, Z −478–1,166, saved surface Y 56–127 (the full sampled-column source range was 20–135).
- 8,192 biome samples plus 8,192 matching height records (16,384 records total). Samples are reference observations, not a complete world scan and not evidence that ungenerated locations have particular biomes.

This fixture is intended for regression tests of planetary biome lookup and saved-surface height calibration. It does not contain blocks or support reconstruction of terrain, structures, or ores.

`seed0_ores.json` contains 256 distinct saved ore-block observations per planet, with registry
block ID, absolute x/y/z and containing chunk x/z. The sample spans 59 Moon and 149 Mars block
IDs. Exact host variants count separately. The feature regression compares the subset whose
block IDs occur in the active vein-output configs within a bounded central crop; a nearby vein
center is candidate evidence, not proof of block-level geometry or host replacement.
