# Native validation

The current Linux build is checked against TFG Core 0.9.25, TFC 3.2.25, Beneath 1.0.6
and Ad Astra 1.15.21. The final release suite passes **571 tests**: 534 library, 15
engine-process, 2 feature-parity, 8 parity, 3 release-CLI, 2 Rocky Plateau, 5 surface-terrain,
1 saved volcanic-lava and 1 saved volcanic-terrain test. `cargo fmt --all --check` passes.
The headless-library check and all-target diagnostics check pass. The official release binary
reports version 0.4.0, target `x86_64-unknown-linux-gnu`, `official: true`,
`diagnostics: false`, and `optional_game_artwork: false` through `--build-info`. Clippy is not a
release gate, so no clean-Clippy claim is made.
This includes Atlas tests covering cache reuse across zoom levels, refinement, family coverage,
adjacent boundaries, split continuation, same-root panning and dimension-aware spawn/player pins.
Saved-location fixtures cover spawn, UUID deduplication, multiple players and dimensions, malformed
files, and both player-name cache formats. Structure regressions cover off-anchor box centers,
negative/inclusive coordinates, start-based identity and deduplication, viewport queries,
close-zoom clustering, search radius/ranking, and centered PNG/CSV/Xaero export destinations. Defaults migrate once and preserve subsequent choices.

Independent layer controls are exercised with real egui pointer clicks, including
adding/removing layers, changing their order, dimension fallback and shared planetary
rendering options. Raster regressions cover blend order, opacity endpoints, saved
canopies below estimated terrain and above water, and keeping relief, contours,
palettes and highlighting visible when terrain detail is enabled.

The complete engine PNG path was compared with 13,824 independently captured World 0
surface columns at six volcanic sites. With a save attached, the tree overlay covers
all 3,145 exposed leaf columns with no extra columns, fixing eleven missed leaves.
The seed-only overlay remains approximate: coverage recall increased from 71.1% to
75.3%, precision stayed about 51%, and F1 increased from 0.594 to 0.607. These sites
include bushes and are not a representative world sample; these scores do not establish
exact tree placement. See the [per-site report](data/tree-overlay-accuracy-world0.json)
and [generation limits](VEGETATION-GENERATION.md). All 22 default tile payloads remain
identical to the preceding build, including biome IDs and water masks.

## Reproducible checks

```sh
cargo fmt --all --check --manifest-path native/terralens-rs/Cargo.toml
cargo test --release --locked --manifest-path native/terralens-rs/Cargo.toml
cargo check --release --locked --no-default-features --lib --manifest-path native/terralens-rs/Cargo.toml
cargo check --release --locked --features diagnostics --all-targets --manifest-path native/terralens-rs/Cargo.toml
```

The normal release has no diagnostic command parser or responsiveness recorder. Separate CLI
checks verify that testing flags are rejected without opening a display; help/version work without
one. `--engine` remains available for the application's isolated background worker process.

The suite covers Java-compatible random streams, biome/climate/forest parity, height error bounds,
ore and structure prediction, read-only save decoding, bounded tile planning, cancellation, process
shutdown, marker grouping, legends, navigation, seed lists, and atomic composed exports. Real child
process checks exercise serialized jobs, saved/predicted provenance, dimension selection and PNG
export without a display.

The expanded seed-zero Overworld audit compares raw candidates against **185,802 completed
chunks** without using saved-start suppression: 450 correct starts, one extra prediction,
and one miss (**0.44% combined disagreement**, 0.22% false positives, 0.22% misses).
All 20 village starts in that cohort match, including Yurts and ruined Norse villages. Both
remaining errors are ocean Moai; its per-type false-positive and miss rates are each 1.05%. The
[full accuracy report](STRUCTURE-ACCURACY.md) records denominators, freshness and limitations.
Flooded-land regressions preserve terrain height and biome identity while keeping every layer,
hillshade, detailed vegetation and connected-region selection consistent with water coverage.
The seed-zero missing Malay example regresses the actual exclusion grid, and mineshaft tests
cover TFG's raw-height rule. Saved-completion bitmaps reject missing/invalid/incomplete metadata.
The river-bank report at **8833, 11107** adds 225 independent saved surface columns: all 189
dry columns and 36 wet columns match the corrected height-only density mask. Separate checks
verify the floodplain shelf profile, all eight map layers and detailed surface rendering while
retaining the adjacent water and the original terrain-height values.

Structure inspection adds native tests for nested TFC vessel inventories, actual Lootr `tileId`
metadata, sharded per-player SavedData lookup, known-empty versus unopened inventories, cross-
dimension/position/UUID rejection, fluid and enchanted-item details, sparse floor cutaways,
and stable inspection caching across zoom and Beneath band changes. A real child-process test
returns structure blocks and vessel contents while checking the save bytes remain unchanged.
The binary palette transport round-trips 160,000 structure blocks below four MiB and rejects
truncated or invalid palette records. Six independent saved starts check seed-selected templates,
rotations and horizontal footprints. Connected layout forecasts reach loot-bearing rooms but
remain explicitly partial; see [structure previews](STRUCTURE-PREVIEWS.md).

Fixtures are committed under [tests/fixtures](../native/terralens-rs/tests/fixtures/README.md) and
`src/beneath/real_biome_fixture.json`. Tests do not need the author's Prism installation or invoke
Minecraft. The observation fixture contains 1,556 quart biomes and 371 climate/forest samples.
The Beneath fixture checks 2,000 saved quart-biome samples. Conversion captures verify fidelity to
the pinned earlier implementation; they are not independent evidence of Minecraft accuracy.

Moon and Mars use the user's existing seed-0 world as an independent reference. All 8,192 saved
biome samples match. Across all 4,096 height columns per planet, Moon mean absolute error is
**0.61 blocks** and Mars **0.68 blocks**; the 95th-percentile error is two blocks for each. Maximum
individual errors are 56 and 28 blocks, respectively. A regression requires mean error below one
block separately for each planet. Exact saved-block decoding confirms the heightmap offset at
64 distributed columns. Java 1.20.1 oracle vectors cover ImprovedNoise's Y smear, resource-seeded
blended noise, biome-boundary tie ordering and all four crater candidate streams. Planetary height
also varies under nonzero and negative seeds. These tests never consult fixtures at runtime.

The planetary surface port includes density caves and Moon crater geodes, with configured cell
heights, ordered feature RNG, biome eligibility and the WorldGenRegion write radius. It does not
simulate every surface-opening carver, decoration or structure block. The 512 saved ore-block
observations provide a bounded candidate-neighborhood cross-check, not block-exact vein parity.
See [planetary generation](PLANETARY-GENERATION.md) for the denominators and limitations.

## Structure applicability

The installed source inventory covers all 66 sets and 131 distinct structure IDs, including
vanilla definitions, effective KubeJS selectors and literal nested biome tags. The 100 km
seed-zero audit stops independently at the first viable placement for each weighted member.
It found every source-compatible type: 30 Overworld, three Beneath, four Moon and zero Mars.
Catalog membership uses an explicit installed-pack compatibility review, independently from
scan results: 30 Overworld, three Beneath, four Moon and zero Mars structure types. Vanilla
villages/strongholds and pack-disabled Primal/Ancient Beasts entries are culled; every TFG
village, temperate house, TFC towerhouse/church/monument variant remains. Portable regressions
check every reviewed registry ID, all five TFG village variants, both towerhouses, dimension
isolation and the exclusion of incompatible families. A source regression verifies that the
pack's explicit Primal/Ancient Beasts `removeAll` calls clear its structure biome tags. Preview biome matching and user visibility
are separate from compatibility. A scan miss cannot change the catalog.

The opt-in diagnostics regression checks all 37 witness locations against production scanners.
Audit and finder use the same biome probes, including Beneath at Y=93, and preserve all children
in the weighted draw. A separate read-only check across four Overworld saves found 1,048 starts
in 453,668 chunks; every recorded registry ID is retained. The pinned report retains independent
evidence for 29 recorded types across Overworld and Moon.

Candidate placement does not simulate successful Minecraft structure generation. Separate
read-only world-0 evidence confirms 25 Overworld types (278 starts) and all four Moon types
(4,295 starts), with bounds and no decoding errors. No Beneath region files exist in that
reference; 13,399 Mars chunks contain no starts. See [structure generation](STRUCTURE-GENERATION.md)
and the [full report](data/structure-audit-seed0-100km.json) for coordinates and limitations.

## Viewer behavior

- The refactored shell was rendered at 940×620 and 1280×800 on an isolated X11 virtual display,
  including the Beneath Y96–111 band. Ctrl+F focused the search field and accepted a query; the
  rail switched to raw ore artwork; Structures and editable analysis coordinates fit the dock.
  Earlier release checks covered result navigation, legends and saved block inspection in both
  dimensions. Heavy jobs remain in a separate process. These checks are not Wayland liveness proof.
- Moon and Mars were opened from world `0` and rendered at 940×620 on an isolated virtual display.
  Biome coverage settled to 100%; each dimension showed its own structure rail, planetary legend
  and saved markers. Portable child-process tests additionally cover their probes, PNG exports
  and saved high/negative sections without requiring that local world or a display.
- The 46px icon rail fits at 940×620, with structure/ore switches stacked above the type icons.
  Right-click expands grouped ores and structures, and child icons control individual variants.
  Catalog partition checks keep each vein selectable once, preserve dimensions and namespaces,
  and verify mixed-vein grouping against configured outputs. Spawn and player artwork stays
  separate from feature clusters and appears in composed PNG/CSV exports.
- Loot-aware grouping reserves Ruins for complete no-container/no-loot evidence. Actual
  small/beach/rich TFC ruins have populated vessel templates and move to visible Ruined
  buildings, with distinct chest-accent art. The audit covers all 131 registered types;
  compact/full-data consistency, complete catalog coverage, unknown handling, grouping,
  default migration, aliases and alpha/icon distinction are checked. A real read-only
  world-0 fixture from chunk 1231,-204 independently proves two populated vessels in the
  mudbrick ruin. All 16 TFC ruined-world templates were separately inspected for absent
  block-entity NBT. See [structure loot](STRUCTURE-LOOT.md).
- Beneath structure pins ignore the selected vertical band in both the map and visible-marker
  exports, while ore pins keep their height filter. A cache regression checks that completed
  seed-placement pins survive repeated band changes and are cleared on a dimension change.
  Requested quiet defaults hide lootless Ruins, Nether fossils, forest roaming, Moon cheese
  and rabbit houses. Migration/serialization tests verify one-time application and subsequent individual
  overrides, including ocean moai and aqueducts in the shared Ruins group.
- The actual dead Norse village remains compatible and groups with Villages rather than Ruins.
  Classification/group tests check its village icon and membership; defaults and migration
  checks restore inherited hiding once while retaining other hidden ruins and later user
  choices. Search aliases distinguish dead/abandoned Norse villages from living villages
  for both saved and seed-derived markers, and the Structures table uses that shared matcher.
- Native icon rendering was checked at 940×620 with 32px transparent structure sprites and larger
  raw ore artwork. Clusters use representative artwork and counts without a disc background.
  Sprite tests cover alpha, the mineshaft opening, TFG symbol routing and distinct vanilla textures
  sharing a structure family. Export tests check transparent icon corners on the map image.
- The planetary ore catalog covers all 27 Moon and 28 Mars vein types with 39 shared mineral
  sprites. Full-catalog checks compare artwork labels with configured primary output weights,
  decode every PNG, require transparent silhouettes, and verify that placed markers across
  dimensions share one GPU texture per mineral. Sources include effective KubeJS item-model
  texture overrides as well as the installed GTCEu color and icon-set definitions.
- Moon cheese, Moon bases and rabbit houses have distinct sprites in the map, icon rail and
  exports. The personal build embeds Ad Astra's local cheese item image; dome and rabbit-nest-box
  artwork is original transparent 32px pixel art. Routing tests cover recorded IDs and placement
  suffixes. The source-only build also compiles without the optional local game image.
- Native asynchronous file picking was opened on the desktop. Portable source fixtures and a real
  engine child test check source import and failure replies; they do not simulate an OS chooser.
  File/folder world IDs agree, and same-named worlds from different locations remain distinct.
- Imports accept arbitrary world, saves and Prism folders. Failed session-source opens retain the
  current document; newer world/seed selections invalidate pending import replies. Structure CSV
  export snapshots the filtered table, and Save As changes the session destination after success.
- Structure map destinations use the midpoint of inclusive saved bounds, including the final
  block's full area. Raw start coordinates remain available for identity and CSV provenance;
  close views (≤8 blocks/px) draw bounded structures individually. This is constant-time per
  marker. A close-view native check used one unchanged mineshaft chunk copied into a temporary
  read-only-source fixture and confirmed the arch at the drawn box center. Unknown jigsaw layouts
  are not materialized and have no fabricated bounds.
- Structure reader fixtures cover arbitrary mod namespaces, dimension-root isolation, and boxes
  whose start point lies outside the visible map. The earlier Ganymede reference save had bounds for all
  356 Overworld and 2,875 Beneath starts, with zero decode errors.
- Cutaway checks prove an Overworld Y0–7 inspection excludes a roof at Y15; Beneath intersects
  structure height with its selected band and leaves missing blocks unknown. Disabled detail
  cannot request or probe chunks. Stale replies cannot mix cutaway contexts.
- Marker regression tests reproduce a pair splitting after a 20-pixel pan with the old screen
  grid. World-anchored grouping and padded queries preserve membership and centroids, including
  viewport edges and negative coordinates.
- Cache-only legend sampling is bounded to 4,096 points. Shares use the loaded-sample denominator,
  and a final repaint refreshes coverage after generation settles.
- Export checks include bounds, clipping, Unicode units, artwork, structure footprints, atomic
  cancellation and headless child-process rendering. A [sample export](native-qol-export.png) is
  included; machine-specific screenshots and raw discovery reports are excluded from publication.

## Map detail, relief and player heads

- Ctrl+G is exercised with real egui Enter key events, checking both a successful coordinate
  jump and invalid input retaining the popup. It uses the same validation path as Go.
- Close atlas rasters use one texel per world block through two blocks per screen point.
  Sub-block zoom reuses those samples with nearest filtering rather than requesting duplicated
  fractional-block tiles. Preview rasters keep linear filtering. Regression checks cover exact
  block stride and shared gutter positions at negative coordinates.
- Optional NW-lit Lambert hillshade overlays any map layer while preserving ocean colors.
  Finite-data, flat-color, light-direction, sample-spacing, border equality, no-height fallback
  and preference serialization/identity checks pass. Eight padded height grids are cached per
  worker using full world/settings/dimension/tile identity; layers and strength reuse the data.
  Cancelled grids are not cached, and Beneath is unchanged. Relief includes ocean floors and
  flooded lowlands while retaining blue water hues; regressions check both paths. It shades
  the terrain-height model and does not generate full Minecraft chunks.
- Player heads extract the 8×8 face and alpha-composited hat from Minecraft skin PNGs. GUI
  texture identity includes UUID and updates on completion; disk and HTTP work stay on one
  bounded worker. Offline cached heads are used by exports. Tests cover UUID handling, cache
  image round trips, alpha compositing, legacy 64×32 hat padding, opaque base faces, strict
  texture host handling and the fallback artwork.
  An isolated end-to-end check fetched a public test account's head in about five seconds and
  reloaded its disk cache; it did not use a user's UUID or require Minecraft/account credentials.

## Volcanic terrain

The centered cinder-cone, tuff-ring and tuya samplers now participate in the shared terrain
height model, after shore/tide blending and before rivers. Original Java sampler fixtures
cover 1,320 volcanic and 2,520 reachable shore cases across five seeds. A separate saved
world-0 fixture requires exact terrain-noise height at every one of 1,334 columns from
five volcanic sites: mean, 95th-percentile and maximum errors are zero in that sample.
Final saved block surfaces are recorded separately and are not claimed exact. Regression
checks cover all 109 biome blend groups, volcanic river profiles and seed/settings cache
isolation. See [volcanic terrain](VOLCANOES.md) for provenance and sample coordinates.

Headless 256×256 shaded Height exports around a cinder cone, active shield vent and tuya
show the changed relief without visible export seams. Close-view exports took approximately
1.1–1.6 seconds, within 3–5% of the pre-overlay build on this machine. A wider ±4,096-block
export took about 4.2 seconds with either build. These bounded checks are not a general
interactive-frame-rate guarantee; generation still runs in the engine child process.

Volcanic lava has two independent saved-block fixtures. The Tuya seed model matches 594 of
595 exposed lava columns, with one extra column and one miss, and no wrong elevations among
matches. This result does not extend to ordinary-cinder seed forecasts. Source integration
uses 23,040 column observations across both fixtures (one control is repeated), and confirms
all 912 observed exposed lava columns render orange on every layer without enabling detail.
A partial-neighborhood test keeps known saved lava/dry columns authoritative while unknown
neighbors retain the seed model. Per-pool Tuya gates, bounded flood/spill tests, source/reload
cache identity, canceled publication, cross-layer reuse, overlay color, plant exclusion and
close-view pin visibility are covered. See [surface lava](VOLCANOES.md#surface-lava) for the
ordinary-cinder forecast and dynamic-fluid limits.
An actual engine-process export with world `0` open at `13664,14159` verified 600 orange
pixels (150 saved lava columns at two pixels per block in each axis) on all eight layers,
with relief and contours enabled and without a detail request. The initial 96×96 view took
0.33 seconds; following layers took 0.09–0.11 seconds in this bounded check. A read-only
reference search found 78 saved centers across seven categories; see the
[reference coordinates](WORLD0-VOLCANO-REFERENCES.md).
Headless biome exports visually checked both caldera types with contours and relief enabled.
The normal-Tuya close view took 1.44 seconds versus 1.33 before lava rendering; a dry-shield
control took 1.36 versus 1.33 seconds and produced byte-identical PNGs. These are bounded
local timing samples, not a general frame-rate claim.

## Terrain contours

Contour lines interpolate the existing sampled terrain heights on a fixed local triangle
mesh. Their elevation interval, major-line frequency, thickness, opacity, color and underwater
visibility persist with display preferences. Flat plateaus are not filled with contour bands;
minor and major line densities fade separately when samples become too coarse. Contours work
independently from hillshade and reuse the same bounded elevation-grid cache in engine workers.
They do not generate chunks or run terrain sampling in the UI.

Eight focused math tests cover flat terrain, negative elevations, independent major contours,
dense slopes, missing/non-finite heights, inactive settings and raster identity. Integration
checks exercise preference round trips, matching contour texels on adjacent negative-coordinate
tiles in both axes and at two resolutions, agreement between atlas and export pixels, and
underwater visibility without changing biome/water data. Height-grid padding includes diagonal
neighbors so strokes join through visible tile gutters. Rectangular PNG exports pass independent
horizontal and vertical sampling intervals to the overlay renderer.

Headless exports around a seed-0 active shield confirm closed crater/slope contours on the
biome and elevation layers, both with and without relief. A 384×384 biome export took 5.37
seconds with contours disabled and 5.43 seconds with them enabled in this bounded check.
The disabled export was byte-for-byte identical to the pre-contour build. A wider view also
kept water clear with underwater contours disabled; these are visual/performance samples,
not a guarantee for every viewport.

## Optional diagnostics

Build with `--features diagnostics` to enable `--info`, `--probe`, `--find`, `--render`, native smoke
captures and latency workloads. For example:

```sh
cargo run --release --locked --features diagnostics --manifest-path native/terralens-rs/Cargo.toml -- --info
cargo run --release --locked --features diagnostics --manifest-path native/terralens-rs/Cargo.toml --example zoom_streaming
```

`zoom_streaming` checks actual fine-tile coverage after pans/zoom transitions, rather than treating
an idle queue as proof of a finished map. The intermittent terrain flicker reported during local
use was not reproduced and is not claimed fixed. The separate Wayland protocol probe under
`tools/` tests background ping replies; ordinary FPS measurements are not used as an OS-liveness
proof.

## Surface caves, sandstone and root placement

The surface pass was verified with 507 passing release tests, including the new
independent saved-voxel/floor regressions and original-Java 3D noise controls.
The final worker-relief cache change reran all 475 library tests, 15 child-engine
checks and three surface-terrain checks. All 22 release-engine tile payloads
matched the pre-cache implementation byte for byte. A headless library build
also passed. Snow/ice are explicitly excluded only where observed in the saved
reference substrate; known ground beneath thin snow remains scored.

See [surface caves](SURFACE-CAVES.md) and [generator accuracy](GENERATOR-ACCURACY.md)
for reference capture tools, denominators, extra voids and tree-placement limits.

## Surface mismatch scenario audit

The follow-up passes all **510 release tests**, including buried-river colors in
ordinary/stacked/colorblind biome maps, explicitly enabled grass colors, dry
low-bank controls, boulders emerging above water, steep river gravel, volcanic
shore basalt and unrounded Y=64 builder boundaries. The seed-only sample improves
from 707 to 573 failures, resolving 134 columns with no newly failing reference
columns. Twenty-two engine measurements and three headless map exports completed;
the UI process was not restarted during verification. See
[the scenario report](SURFACE-MISMATCH-SCENARIOS.md) for exact coordinates and
[engine timings](data/surface-scenarios-engine-performance.json) for measured scope.

## Lake and plateau follow-up

All **516 release tests** pass after the lake color correction, density-context
reuse and Rocky Plateau raw-rock dispatch. The additional two independently
captured World 0 chunks match all 512 natural surface blocks and elevations,
correcting 256 wrong stone columns and preserving the mixed soil/rock control.
The fixed 48-chunk accuracy metrics are unchanged. A headless library check,
22 engine cases, four material timing/parity cases, five normal-engine PNG
exports and a read-only fixture recapture also pass. Seven biome payloads change
with the lake swatch correction; every non-biome payload and all four material
payloads match the prior release. No game/save writes or UI restart were used.
The separate volcanic grass prediction error is reproduced and remains open;
see [the follow-up evidence](SURFACE-FOLLOWUP.md).

## Calendar, teleport heights and exposed rock follow-up

The simulated-date controls retain Average as the default, preserve physical
terrain/marker caches and recolor map/detail/export output. The numeric color
sampler matches 385 independent Java controls; calendar changes leave physical
probes unchanged. See [calendar colors and teleport heights](SEASONAL-COLORS.md).
Teleport export requests exact destination probes asynchronously and uses the
resolved ground/fluid plane plus one for player feet, retaining explicit marker
Y and Beneath slice behavior.

The rock follow-up fixes eight saved diorite columns and one spring-material
column in the immutable spread reference, with zero new surface failures.
The user-supplied spring neighborhood is measured separately: 159/163 exposed
rock materials and 2,144/2,304 complete surface block/elevation pairs. Its 156
water columns remain missing from the seed-only projection. The saved-world
path is checked independently across every layer, including all 28 plant-covered
water columns and the surrounding dry rim. See [spring limits](HOT-SPRINGS.md).

Dike replay excludes Core's basal hornfels band, uses a shared bounded surface
cache, and drops only the RNG tail after the last possible exposed write. An
independent 512-column saved-block control validates basal exclusions. A separate
regression compares complete replay with the shortened traversal over four
reference chunks and requires real exposed writes. Neither control claims that
all preceding ore/erosion/vegetation state is implemented.

The shipping source passes **543 release tests**, plus headless-library and
all-target diagnostic checks. The full-replay/prefix regression passes in the
same suite. The installed binary is built with the normal default features;
maintainer diagnostic commands remain disabled.

The final normal engine completes 22 terrain/dimension cases, four material
cases, four targeted before/after tile cases and eight PNG exports. Calendar
exports take 0.233 s initially and 0.004–0.069 s thereafter in this local sample;
reset restores the original PNG byte for byte and every physical probe remains
identical. At the exposed dike tile, cold work is 2.17 s versus 1.63 s before the
new dike pass; the following Ground-layer request is 0.918 s with either build.
At the user spring tile, cold work is 1.91 s versus 1.87 s, and the following
Ground request is 1.02 s versus 1.01 s. These are child-engine timings, not GPU
frame rates or OS-liveness measurements. Regional material sampling remains a
larger workload (9.78 s cold in this run), so these figures do not claim that all
uncached viewports generate immediately. Reports:
[engine cases](data/rock-spring-engine-performance.json),
[material cases](data/rock-spring-material-performance.json),
[targeted comparison](data/rock-spring-performance.json).

## Optional terrain profiles and surface litter

The final suite above includes the terrain-profile and surface-litter regressions.
A focused run also passes all 20 preference tests after the legacy-session
migration change. Accuracy controls now explicitly request their original full
sampler; dedicated Fast controls verify zero height, volcanic and cave queries,
tile/export agreement, cancellation, basic hover and explicit teleport height.
Quality changes preserve completed structure/ore scans.

Independent ZIP resource controls verify the installed random-patch constants
and nested loose-rock heightmap. Production saved litter is checked against 53
positive independently decoded columns across four World 0 chunks, covering
actual loose/mossy rocks, twigs, leaves, sticks and other groundcover. Further
controls verify negative-coordinate clipping, texture/tint composition, water
and coarse-scale exclusion, unchanged tree output and cache isolation. The seed
projection’s separate measured limits are recorded in [surface litter](SURFACE-LITTER.md).

Eighteen child-engine timing/payload measurements and six normal-engine PNG
exports completed. Fast local/medium biome tiles improve by 4.58×/8.28× in this
bounded sample; all six full-terrain historical controls retain identical bytes.
See [performance](data/terrain-profile-comparison.json). The normal binary keeps
maintainer diagnostics disabled. The running user app was not restarted, and no
Minecraft process was launched or save modified.
