# Native parity fixtures

`ground/seed0-ground-samples.json` captures all 2,048 columns of eight fixed World 0
chunks, including 836 columns covered by foliage, plants, loose rocks, snow layers
or other blocks. The corresponding `.nbt.gz` snapshots retain only original block
state sections and chunk status, with all entities, containers and save metadata
omitted. `tools/reference/extract_ground.py` decodes columns independently of Rust,
handles fluid inside cover and records stable source-region hashes. Offline reader
tests check every selected block and Y, and a temporary-region test checks map
colors. This validates saved extraction, not seed-only ground generation accuracy.

`ground/rocky-plateau-rock-seed0-1550-704.json` and
`ground/rocky-plateau-seed0-1536-704.json` add two complete saved plateau controls:
256 raw rhyolite columns, and 256 mixed soil/rock columns. They come from the
independent Python palette decoder and retain the source-region hash. Runtime
tests supply only the seed to the sampler and compare every block and elevation.
Regenerate read-only with `tools/reference/extract_rocky_plateau.py /path/to/world`.

`ganymede-observations.json` contains independent data from the installed Minecraft save:
1,556 stored quart biomes, 371 TFC chunk climate samples, 371 forest ordinals, 371 generated
surface heights and 370 observed raw-rock blocks. The seed is a decimal string to preserve
all signed 64-bit bits. Captures read saves without changing them. Region digests belong to
the original biome/climate capture; forests, heights and rocks were read from the same save
again during the Rust port. Height remains an approximation with the previous measured
95th-percentile error of five blocks and maximum error of 19 blocks.

`tfg-reference.json` is a conversion reference captured from the existing TypeScript
generator, **not** independent Minecraft evidence. It covers three seeds, 274 locations per
seed, signed chunk/quart/grid boundaries, positive and negative coordinates, full probes,
and three 512-by-512-block feature windows (715 deposits and 12 structure candidates).
Source SHA-256 values identify the relevant source/data inputs. This file is immutable test
input: Rust tests do not invoke JavaScript or regenerate expected results.

The TypeScript capture originates from [OutCrop](https://github.com/PauAixala/outcropmap),
pinned at `7d5b7940d96cbe134001416b09ea13862dba2598`. The original fixture metadata retains
source hashes and provenance. Regeneration is a maintainer task performed against that reference;
normal tests use only the committed data and require neither Node nor Minecraft.

The JVM/Minecraft RNG captures are vendored in `core/`; their `source` fields record the Java
capture. Climate gates compare exact float bits, and biome/forest/geology conversion gates
compare exact outputs. The old TypeScript absolute heights are retained as historical data;
they do not override the direct TFG terrain port. Feature conversion tests use controlled
terrain inputs to retain placement witnesses independently from changed terrain and host-rock
gates. Gaussian streams alone permit a small transcendental-library tolerance.

`tfg-centered-volcanic-java.json` captures the unmodified Core 0.9.25 centered-feature and
shore sampler classes, loaded with their original noise and Minecraft math dependencies.
Only biome registry metadata is stubbed. The fixture records the source commit and mapped
jar SHA-256. Its 1,320 volcanic and 2,520 reachable shore cases span five signed seeds,
biome controls, synthetic input heights and shoreline edge cases; additional zero-shore-weight
calls record Java's behavior outside the filler's reachable dispatch. The maintenance script
is `tools/reference/centered_oracle.py`. `tfg-centered-volcanic-height-source.json` separately
pins shape expressions and biome controls to that source.

`volcanic-terrain/seed0-generated-columns.json` contains 1,334 independent saved TFC
terrain-noise heights at five volcanic sites, with saved block surfaces recorded separately.
Each site covers all 256 center-chunk columns plus available cardinal rim/control samples.
Selection used generated-chunk availability rather than model agreement. Source-region hashes
and read-only extraction details are embedded in its provenance. Every terrain-noise height
must match exactly; this does not assert exact final block surfaces or universal height parity.

`volcanic-terrain/seed0-surface-fluids.json` independently records the highest saved non-air
block, Y, surface-biome name and exposed-lava flag at all 11,520 columns in 45 complete chunks
around five volcanic centers. It includes two lava-bearing calderas and three dry controls.
Source-region before/after hashes and the extraction method are embedded. No model output
selected the columns. Lava precision, recall and level checks use the positive lava-column
denominators rather than treating the numerous dry columns as accuracy evidence.

`volcanic-terrain/seed0-water-magma.json` records all 13,824 highest non-air columns in
six complete 3×3-chunk neighborhoods: three water-filled cinders and three lava/magma
cinders. It includes 760 water, 184 exposed magma and 317 exposed lava tops, with exact
block IDs and elevations. Every column is retained, including plants and dry controls;
region hashes stayed unchanged during extraction. The read-only maintenance script is
`tools/reference/extract_volcanic_water_magma.py`. This fixture checks saved-source surface
rendering independently of seed-only placement accuracy.

`generator-accuracy/world0-surface-profiles-48.json.gz` is a read-only,
independently decoded reference of a 4×4 vertical lattice per spread chunk plus
all natural surface columns. It preserves explicit unknown/frozen exclusions
and region-file SHA-256 fingerprints. `surface-noise-java.json` captures 512 calls
to the installed original TFC simplex/cellular classes over four seeds; regenerate
it with `tools/reference/surface_noise_oracle.py`. Neither fixture supplies
blocks to the seed-only World under test.

`ground/dike-user-site-seed0.json` is the independent nine-chunk, 2,304-column
World 0 capture around the user's `-319262,16107` reference. The nearby quartzite
patch is a spring basin rather than an intrusive dike. It includes all observed
natural materials, top cover, elevations and the stable source-region hash.
`tools/reference/extract_dike_site.py` refreshes it read-only. The seed-only
basin score and saved-source fluid-mask test have separate denominators; the
fixture must not be used to claim exact seed-only water prediction.

`ground/basal-rock-world0.json` captures 512 complete basal columns across the
exposed-diorite and spring reference chunks, with 7,168 saved block observations.
The source-noise exclusion test checks that every modeled hornfels/bedrock
exclusion is an observed non-host block. It does not claim to reconstruct all
other ore/carver replacements in that band. Refresh read-only using
`tools/reference/extract_basal_rock.py /path/to/world --output /tmp/basal.json`.

`ground/litter-rules.json` directly decodes the installed Core jar’s five
stick/salt/dry-grass patch chains plus surface loose rocks, retaining source JSON,
child heightmap modifiers and resource/jar hashes. It is independent of Rust rule
constants and guards their placement counts/spreads and modifier ordering.
The existing ground NBT snapshots also supply 53 observed litter-positive top
columns across four saved chunks for the production overlay-reader tests.
