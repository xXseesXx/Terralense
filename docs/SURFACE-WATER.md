# Surface water in the Overworld

The atlas checks whether ordinary height-noise terrain leaves room for water at the sea
surface. TFC's sea-level aquifer fills air blocks strictly below **Y=63**, so its highest fluid
block is **Y=62**. The sampled height is a noise input, not the highest solid block. A direct
`height < 63` test floods dry river banks and is no longer used. Ocean colors remain unchanged.
River and lake biome names do not force a column to be water: their dry banks and terrain
above sea level keep land colors. The underlying biome identifier and terrain height
remain available; flooded lowlands are still lowlands, not renamed ocean biomes.

For ordinary height-noise columns, TFG starts with solid density 0.4 and applies its upper slide
above the integer noise height. The viewer evaluates that density at the top fluid block,
preserving Java's promoted `0.2f` coefficient. A noise height of 61 leaves solid terrain at Y62;
a noise height of 60 leaves air that the sea-level aquifer can fill. The terrain-height model
used by structure placement is unchanged.

Water keeps its own blue palette on all map layers, including climate, forests, geology and
elevation. Optional hillshade varies its brightness using the terrain below the water, without
flattening that terrain to sea level. Land highlights do not recolor it. Cached water coverage is shared across layers and
sent with completed map tiles, so connected-region highlighting cannot bridge flooded columns.
The original biome IDs remain available to the legend. Detail surface previews show the water
surface and prevent ordinary trees and crops from overwriting it. Saved detail blocks continue
to show the actual recorded world, including aquatic plants and any local variation.

The read-only reference at seed 0, chunk `(991, 990)`, has terrain height 59 and fluid tops at
62 despite having a lowlands biome. Of its 256 surface columns, 124 end in fresh water, 52 in
salt water, and 80 in aquatic plants. The fixture is stored under
`tests/fixtures/flooded-lowlands/seed0-991-990.json`.

At the reported **8833, 11107**, the river's floodplain profile produces a 61.5-height shelf
(integer noise height 61), while the saved grass surface is Y62 with no water. The neighboring
**8827, 11107** has noise height 60, gravel at Y61 and surface water at Y62. A separate saved
15×15 observation fixture covers **225 columns: 189 dry and 36 wet**. The corrected mask matches
all of them, including a TFC loose-rock block containing water; underground cave water does not
count as surface water. A river-profile regression also verifies that the floodplain shelf and
lower bed already exist in the height model outside the river biome itself.

This surface-water rule remains an approximation over the existing seed height model. It does
not reproduce special carving samplers, aquifer borders, caves, tidal construction, ice or every
shoreline block. Structure generation
uses its own source-defined biome and terrain gates; a water-colored map pixel alone is not
evidence that every structure must fail there.

Dry river/lake columns use their ground material on the biome map, while the water overlay
leaves terrain visible there. Climate, elevation, forest and geology layers sample their normal
land values. At close zoom (up to four blocks per pixel), saved ground can correct river/lake
coverage in every layer, including actual water above sea level. Modeled volcanic water remains
visible at its own elevation. Biome IDs stay unchanged for hover, search and highlighting.

Seed-0 regression locations include a river at `(10880, -16000)` with terrain noise height 146,
an old mountain lake at `(10752, -15616)` with height 85, and a lake at `(12800, -15232)` with
height 68. These are seed-model witnesses rather than saved-block observations; those chunks
are absent from the reference save. Tests also check tile/export agreement and the recorded wet
river controls near `(-50, 80)` and `(8780, 10992)`. A separate saved-ground input test keeps
elevated water blue while its neighboring dry lake-biome column remains land.
