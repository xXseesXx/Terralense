# Beneath (Nether) generation reference

TerraFirmaGreg calls the dimension **The Beneath**, but its saved dimension key is
`minecraft:the_nether`. In the installed pack, that dimension uses type
`tfg:the_nether` and a `kubejs_tfc:wrapped` generator with event key `nether`, wrapping
a vanilla `minecraft:noise` generator. Its biome source is `minecraft:multi_noise`.
The sources are the installed instance's `kubejs/data/minecraft/dimension/the_nether.json`,
`kubejs/data/tfg/dimension_type/the_nether.json`, and `kubejs/data/tfg/worldgen/noise_settings/nether_noise.json`.

## Biome sampling

The noise settings set `legacy_random_source: true`. Reproducing the biome sampler therefore
requires Minecraft's legacy random path, rather than Xoroshiro: initialize the legacy source
from the world seed, fork a positional factory, and derive each named noise from the Java
`String.hashCode()` of its ID XORed with the factory seed. The generator replaces
`minecraft:offset` with a zero-amplitude noise on this legacy path, so the climate router's
shifted-noise functions sample unshifted coordinates.

Each named normal noise uses two independently seeded Perlin stacks. Seed active octaves with
`octave_{firstOctave + index}`, skip zero-amplitude octaves, and use an input scale of
`2^firstOctave`. The second stack samples at `1.0181268882175227` times the coordinates.
These details matter: treating the offset as a real noise or reversing the first-octave sign
produces plausible-looking maps that do not match saved biome palettes.

The climate router yields six values in this order: temperature, vegetation/humidity,
continents, erosion, depth, ridges/weirdness. Its relevant noise scales are 0.6 for the
bottom, middle, top, and depth noises, and 0.35 for the middle and top forest noises. The
depth axis adds `y_clamped_gradient` (2 at Y=42 to -2 at Y=170) to the depth noise. Minecraft
quantizes each climate value by truncating `value * 10000`, then chooses the nearest biome
parameter box across all six axes, including its offset penalty.

The exact noise parameters and router were extracted to
the pinned OutCrop Beneath configuration. The installed raw noise files are under
`kubejs/data/tfg/worldgen/noise/nether_*.json`; biome parameter boxes are in
`kubejs/data/minecraft/dimension/the_nether.json`. The router has 17 parameter entries
representing 11 unique biomes. Because depth depends on Y, the biome map must keep a selected
Y. The viewer uses section-aligned 16-block layers (0–15 through 240–255), samples biomes
at the middle of each layer, and shows the highest saved block within that layer in detail mode.
The upper preset selects 192–207; lower and middle layers show different biome bands.
Structure pins span every band, including saved starts outside the selected height. Completed
placement caches survive band changes; only the terrain raster and height-dependent detail are
reloaded. Ore markers are filtered by their vertical extent. Nether fossils start hidden and
can be enabled through the structure rail.
Biomes are stored at quart resolution, so exact saved palette comparisons use
quart-aligned X, Y, and Z.

## Height and saved data

There are three different height values in the pack and they have different meanings:

| Setting | Value | Source and meaning |
| --- | ---: | --- |
| Dimension type `min_y` / `height` | 0 / 256 | The dimension's vertical bounds. |
| Noise settings `noise.min_y` / `noise.height` | 0 / 208 | The configured noise-generation volume. |
| `global.NETHER_HEIGHT` | 208 | TFC chunk-data provider's flat rock-metadata surface reference. |

`kubejs/server_scripts/tfg/worldgen/events.chunks.js` sets the Nether's TFC temperature and
rainfall layers to zero, assigns the fixed 208 reference height to its chunk data, and samples
rock through a lateral layer plus depth noise. This is TFC metadata; it does not make the
generated terrain columns flat. The underlying noise settings control terrain. Keep the
selected biome Y explicit. The viewer allows the full 0 through 255 dimension range; its
lower/middle/upper presets sample the generated volume. Extra vertical range is not evidence
that terrain generation fills it.

Saved chunks are read-only evidence for already generated locations. The detail view can show
saved block cutaways within a layer; unseen terrain, caves, and surface blocks are not reconstructed
by the biome sampler. The map's biome raster is seed-derived. Saved biome palettes are useful
for validation, but are not currently the source of the map raster.

## Validation and provenance

`native/terralens-rs/src/beneath/real_biome_fixture.json` contains 2,000 quart biome samples from a
real Nether save, spanning 230 chunks and 36 regions, with quart Y values 0 through 63. The
fixture seed is `-6696614430994881185`; it is a separate save, not the user's Ganymede world.
The fixture covers 11 unique biomes:

| Biome | Samples |
| --- | ---: |
| `tfg:nether/muggy_bog` | 440 |
| `tfg:nether/lava_floes` | 337 |
| `tfg:nether/ash_forest` | 227 |
| `tfg:nether/thorny_brambles` | 187 |
| `tfg:nether/webbed_lair` | 164 |
| `tfg:nether/geothermal_springs` | 161 |
| `tfg:nether/decaying_caverns` | 159 |
| `tfg:nether/salt_caves` | 99 |
| `tfg:nether/basalt_deltas` | 97 |
| `tfg:nether/lush_hollow` | 84 |
| `tfg:nether/tar_pools` | 45 |

The Rust sampler matches all 2,000 stored palette samples for that fixture seed. It validates
the algorithm against a generated world independently of the earlier TypeScript conversion.
Ganymede's `level.dat` identifies seed
`7787531943268589495` (DataVersion 3465, Minecraft 1.20.1), and its Nether folder is `DIM-1`.
The local region directory contains 22 nonempty `.mca` files and 13,032 nonzero location
entries in their headers; header entries alone are not proof that every chunk has a complete
biome palette.

The read-only Rust `beneath_validation` example also checks the current Ganymede save directly.
It samples the 22 nonempty region files at Y=20, 96 and 195. Six chosen chunks lack complete
biome data at all three heights; the other 16 provide **768 quart comparisons, all exact**.
The saved-floor sample near the coverage center uses chunk `(-64,-128)`: its Y≤96 view has
256 exposed columns spanning Y=42 through 93, with basalt, deepslate, lava, magma, vegetation
and ore indicators. The run took about 17 ms for biome comparisons on this machine. These
live-save observations complement the permanent 2,000-sample fixture.

Saved structure starts retain their inclusive bounding box, falling back to the union of
recorded child-piece boxes. Map footprints use those actual bounds. Detail mode reads the
structure's saved blocks together with the surrounding terrain in the selected 16-block layer.
Seed-only structure candidates have no fabricated bounding boxes or block geometry: their
exact assembly still depends on structure RNG, jigsaw pools, orientation and terrain adaptation.

```sh
cargo run --release --locked --manifest-path native/terralens-rs/Cargo.toml --example beneath_validation
terralens --dimension beneath --slice-y 96
terralens --probe --dimension beneath --slice-y 20 --x -1024 --z -2048
terralens --find Ganymede --dimension beneath --scope ores --query sulfur --radius 2048
```

This work supports exact biome-sampler prediction and saved-block inspection. It does not
rebuild the full Minecraft density pipeline, carvers, aquifers, surface rules, or all placed
features for unexplored chunks. Ore and structure markers should be described as seed-based
predictions where their own generators are ported, and as saved discoveries where read from
the world; neither implies a complete 3D Nether reconstruction.
