# Moon and Mars generation

TerraLens supports the installed TerraFirmaGreg Modern Moon and Mars profiles in native Rust.
Choose **Moon** or **Mars** in the dimension selector while browsing a seed or after opening a
compatible save. Planetary rendering and searches run in the separate engine process.
Saved files remain read-only.

## Profile and dimension isolation

The pinned profile is Minecraft 1.20.1, TFG Core 0.9.25, TFC 3.2.25 and Ad Astra 1.15.21, with the
pack's KubeJS overrides. Both dimensions use `kubejs_tfc:wrapped` around `minecraft:noise`; these
are not stock Ad Astra worlds. Moon uses `tfg:moon_noise`, Mars `tfg:mars_noise`. Imports require
the corresponding wrapper/noise-setting combination. Orbits, Venus, Mercury and the other pack
dimensions remain unsupported.

Moon's active dimension type is `tfg:moon`, with physical Y=0..383. Mars's active type is
`ad_astra:mars`, with physical Y=-64..319. Its noise settings instead start at Y=-32 and extend
to 351; the noise grid and physical world bounds are separate. The unused TFG Mars dimension-type
document does not change the active world's build limits.

Planetary samples, searches, saved chunks and marker caches are isolated by dimension.
TFC Overworld rainfall, temperature, geology, forests, tree estimates and base-location analysis
are unavailable on planets. Beneath alone uses the 16-block cave bands.

## Biomes and terrain

The biome sampler implements Minecraft's Xoroshiro positional random factories, resource-name
hashing, ImprovedNoise, Perlin/NormalNoise, density arithmetic and quantized multi-noise lookup.
Moon has ten biome IDs; Mars has fourteen and 27 climate entries. Shared-endpoint ties use a
canonical cold-cache ordering exported from the actual 1.20.1 `Climate.RTree`. Minecraft's last-leaf
thread cache can make equal-fitness boundary choices history-dependent; TerraLens keeps maps
stable regardless of the order tiles load.

The terrain sampler evaluates the installed density-router resources, including old blended
noise, shifted noise, splines and vanilla cave dependencies. Height is a surface estimate,
not a full block generator. Noise initialization is lazy and sampled columns and noise corners
are cached. Saved close-up terrain uses actual block states, including structures and decorations.
Moon height additionally applies the pack’s four air-filled crater geode features with vanilla
decoration RNG and biome eligibility. Surface-opening cave/canyon carvers and later decoration
are not fully simulated. Missing planetary chunks retain their biome map; trees, crops, aquifers,
surface replacement blocks and jigsaw layouts are not fabricated.

The exact wrapper JSON, dependency resources, versions and source hashes are recorded under
[`src/planetary/assets`](../native/terralens-rs/src/planetary/assets/README.md). Runtime requires
no Minecraft installation, Java process or game launch.

## Ores and structures

The catalogs contain the active 27 Moon and 28 Mars TFC vein configurations. Native vein-start
sampling follows configured cluster, disc and pipe RNG, rarity, Y and geometry settings. Markers
are searchable by vein and mineral contents and grouped by their configured primary output.
Existing matching raw-mineral artwork is reused; unknown minerals are not given unrelated art.
Replacement host blocks are not checked throughout each vein, so a start alone does not prove
that every configured ore block exists there.

Structure sets respect their biome gates. Stock Ad Astra Moon dungeon and Mars temple tags do
not overlap the active TFG biome IDs; those sets yield no starts rather than allowing unrelated
biomes. Recorded structures from every namespace remain available independently. Their saved
bounding boxes, centered markers and block cutaways use the existing structure controls.

## Reference validation

The user's world `0` is seed **0** and contains generated chunks in both dimensions. The committed
reference has 4,096 surface quart-biome samples and matching saved height observations per planet.
All **8,192 biome samples match**. The saved Moon area contains all ten biome IDs; the saved Mars
area contains eight of its fourteen possible IDs.

Heightmap extraction was independently checked against decoded block states at 64 distributed
columns: `top solid Y = stored WORLD_SURFACE value - 1 + dimension minimum Y`. All **4,096 columns
per planet** measured Moon elevation MAE **0.61 blocks**, 95th-percentile error **2**, maximum **56**; Mars MAE **0.68 blocks**, 95th-percentile error **2**, maximum **28**.
A regression requires mean error below one block separately for each planet. Saved modifications and
decoration can affect these observations; they are not a claim of block-exact terrain generation.

The fixture also contains 256 saved ore-block observations per planet. In a bounded 0..1024
block crop, all 39 Moon and 71 Mars observations whose exact block IDs occur in the active vein
outputs have a compatible vein within 64 horizontal blocks and its configured Y extent. This
checks a candidate neighborhood, not exact ore-block placement. These observations are
used for validation and are never consulted by the runtime generator. Other seeds use the same
algorithms, with no seed-0 coordinate corrections.

Portable engine-process tests exercise both planets' probes, biome-map PNG exports and saved
surfaces. Separate fixtures cover high Moon and negative Mars sections, tints, nearest-biome
searches, cancellation and dimension-specific catalogs. Tests do not launch Minecraft or write
to the source save.
