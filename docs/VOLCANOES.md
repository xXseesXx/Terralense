# Volcanic terrain and landmarks

The Overworld Structures rail contains a Volcanoes group. Click its icon to toggle all types;
right-click to expand individual vent, crater and tuya controls. These choices persist.
Ctrl+F has a Volcanoes scope; Everything also recognizes volcano, volcanoes and volcanos.
Results use the search point and radius and sort by horizontal distance. Markers and PNG/CSV
exports share the same positions and artwork. Common cones disappear at a closer zoom-out
level than the rarer volcanic features.

Volcanoes use their own marker kind. They never enter the structure-start catalog, structure
absence checks, loot inspection or bounding-box/cutaway pipeline. Reading a completed chunk
without a structure start cannot remove a volcanic terrain marker.

## Installed generation contract

The source profile is TFG Core Modern 0.9.25 for Minecraft 1.20.1. `CenteredFeatureNoise`
selects seeded cellular feature points, checks the raw quart biome at the integer center,
and accepts the cell when `abs(cell.noise) <= 1 / biomeRarity`.

| Family | Cellular spread | Jitter | Eligible terrain |
| --- | --- | --- | --- |
| Cinder | `0.009f` | default `0.43701595f` | Active shield vents, canyon and volcanic mountain cones |
| Tuff ring | `0.003f` | `0.2f` | Dormant, extinct, ancient, sunken and coastal shield craters |
| Tuya | `0.0033f` | `0.21f` | Tuyas and ice-sheet tuyas, including their edge biome |

The Rust index enumerates exact lattice feature points, preserving Java's float parameters,
seed folding, hash and center cast. It does not sample screen pixels to discover sites.
Thus coordinates and instance IDs stay fixed across pans, zooms and adjacent feature tiles.
Scans are cancellable and have a work budget; incomplete searches report their limit.

## Scope and limits

These pins represent centered volcanic terrain. They do not guarantee a lava pool, an eruption
or a placed caldera's final blocks. TFG's large regional shield terrain and hotspot age fields
already contribute to the biome and height models; individual hotspot grid cells are not
invented as additional volcano sites.

## Terrain height

The Rust height model applies the installed generator's centered cinder-cone, tuff-ring and
tuya samplers, including icy tuyas, active-shield special handling, crater gaps and surface
roughness. Every sampler receives the same incoming terrain height; the generator combines
their present results by maximum, using the original height only when no sampler is present.
This preserves both raised slopes and lowered crater interiors.

The pipeline follows `TFGChunkHeightFiller`: biome-height blending, shore and tide adjustment,
centered volcanic terrain, river carving, then Java-compatible integer truncation. Biome blend
groups, shore base elevations and volcanic river profiles come from Core 0.9.25. The same
height feeds the Height layer, shaded relief, water coverage, surface previews and structure
height conditions. Marker Y is the terrain-model height at the marked center, not an assumed
summit. Terrain work remains in the separate engine process; center-biome samples have a
bounded cache, and height samplers are isolated by both seed and world settings.

The unmodified original Java sampler classes provide 1,320 volcanic reference cases and
2,520 reachable shore reference cases across five seeds. This checks noise, Java float
promotion, signed coordinates, biome controls and shoreline corner cases. The optional
maintenance script is `native/terralens-rs/tools/reference/centered_oracle.py`; ordinary tests
use its captured fixture and do not require Java or a Minecraft installation.

A separate read-only world-0 fixture covers **1,334 generated columns** at five sites:

| Site | Center X, Z | Columns | Terrain-noise height error |
| --- | --- | ---: | ---: |
| Cinder cone | -1264, 4689 | 268 | 0 blocks |
| Active shield vent | -2754, 4513 | 263 | 0 blocks |
| Tuya | 5518, -6365 | 267 | 0 blocks |
| Extinct shield crater | 19624, -3052 | 268 | 0 blocks |
| Glacial tuya | 49954, 28225 | 268 | 0 blocks |

Each site includes all 256 center-chunk columns and available saved rim/control columns out
to 256 blocks. Sites were selected by generated-chunk availability before comparing heights.
Tests require exact agreement at every column with the save's TFC `surfaceHeight` data;
mean, 95th-percentile and maximum errors are all zero in this sample. Fixture provenance
records the source-region hashes. The older height model's errors at these sites are retained
for comparison.

These are terrain-noise elevations, not the final highest solid or fluid block. Caves,
decorations, structures, lava and water can make the saved block surface differ. The fixture
records those surfaces separately. Saved block detail, when enabled, still reads actual blocks;
this sample does not establish block-exact terrain everywhere or across every volcanic subtype.

Validation includes the installed Core Modern cellular reference, signed lattice coordinates,
a fixed seed-0 active vent at X=-2660, Z=3951, full-area versus tile-union equality, dedicated
search/visibility behavior and separation from saved structure absence/inspection.

## Surface lava

Volcanic lava renders as orange surface patches on every main map layer and in PNG exports.
It does not require the detail overlay. Ordinary cinder cones use the installed pack's KubeJS
volcano features; normal and ice-sheet tuyas use their caldera features. Hover probes carry
the fluid's top-block Y separately from the terrain-noise
height. Detailed seed surfaces use lava blocks at that level and exclude plant annotations
there. The fluid retains its bright orange appearance under colorblind palettes, highlights,
shaded relief and contour overlays. Basalt surface-flow materials do not imply exposed lava.

When a world is open, fully generated saved surface chunks around volcanic sites take
precedence. This includes narrow lava outlets that a height-only basin cannot recover. These
bounded reads run in the engine, inspect at most nine chunks per candidate site, and reuse a
worker-local cache. Missing/incomplete neighborhoods retain the seed model. Source identity
and reload revision are part of raster-mask cache keys, so opening/reloading world 0 cannot
reuse a seed-only mask or a mask from another save with the same seed.

Unselected volcano icons fade between four and one block per screen pixel; close views show
the actual lava raster instead of covering it with a 32-pixel sprite. Selected pins remain
visible, and zoomed-out navigation keeps the normal icons. The icon rail and cached feature
positions are unchanged.

The effective cinder feature chooses an active cone with weight four and a dead cone with
weight one. Active cones run magma rivulets before caldera filling, then fissures and a possible
encased lava spout. The model follows this sequence with the bundled canonical feature order;
live feature ordering, caves, vegetation and subsequent fluid ticks can change its result.
Ordinary-cinder seed forecasts are therefore less exact than the Tuya sample below. Saved
surface data remains authoritative where available; the earlier Tuya-only accuracy result is
not extended to all ordinary volcanoes.

The installed `tfg:earth/tuya_caldera` feature selects the seeded Tuya center, then runs TFC's
`FloodFillLakeFeature` with lava, water replacement and overfill enabled. The Rust model uses
the same center/rarity/biome controls, a 44×44 allowed area with the exterior neighbors sampled,
and the minimum 20-block fill volume. A bounded minimax traversal finds the basin's spill
elevation; a connected fill below that elevation gives its footprint. This is the original
flood-fill behavior over solid terrain columns. Both successful and rejected basins are cached;
ordinary map samples exit before terrain sampling when outside a possible caldera's bounds.

A separate read-only saved-block fixture covers all **11,520 columns** in 45 complete chunks
around five volcanic sites, without selecting columns by model agreement:

| Site | Saved lava columns | Matching columns | Extra | Missed | Lava level |
| --- | ---: | ---: | ---: | ---: | ---: |
| Tuya, 5518, -6365 | 173 | 172 | 1 | 1 | Y92 |
| Glacial tuya, 49954, 28225 | 422 | 422 | 0 | 0 | Y96 |

All 594 matching lava columns have the correct fluid level. Precision and recall are each
99.83% across the two observed lava pools; tests require at least 99% separately at each site.
This is a bounded surface comparison, not a universal block-accuracy claim. Caves, snow,
decoration, fissures, small lava springs and later fluid updates can change the final surface.
Small springs and scheduled flow changes are not fully simulated by the seed model. Region
hashes and read-only extraction provenance
are embedded in `seed0-surface-fluids.json`; the maintenance extractor is
`tools/reference/extract_volcanic_surface_fluids.py` in the Rust crate.

An additional world-0 fixture contains three ordinary cinder cones with exposed lava at
`13664,14159`, `-14048,40773` and `-49483,92583`, plus two water/dry controls. The observed
lava-column counts are 150, 28 and 139. Across both fixtures, saved-source integration checks
all 23,040 column observations (including a repeated dry control) and all 912 observed exposed
lava columns on every map layer. This checks the source-to-raster path, not seed-only
prediction accuracy. The additional extractor is `tools/reference/extract_cinder_fluids.py`.

[World-0 reference locations](WORLD0-VOLCANO-REFERENCES.md) cover all seven categories found
in the saved area, including examples with lava and with water.

## Water-filled craters and exposed magma

Dead cinder cones use the installed weighted selector's water branch. The model advances
past the dirt disks, carves the six obsidian rivulets, fills the caldera with water to its
spill level and adds exposed spring-water fissure sources. These surfaces render blue on
every map layer. Magma rivulets on active cones render a darker orange (`190,71,24`), distinct
from lava (`245,107,28`). Fluid that covers magma takes precedence. Magma remains solid in
water coverage calculations and supports no tree or crop annotations. Hover height and
optional block detail use the surface's own elevation without changing terrain-noise height.

Saved coverage spans the owner's full 3×3 chunks (48×48 columns), including the outer
rivulet strips. The caldera algorithm still uses its original 44×44 fill bounds. Saved
water, salt water, spring water, lava and magma tops override forecasts independently per
completed column; saved dry or vegetated columns suppress them. Saved tuff-ring surfaces
also participate, without assuming that every closed tuff crater receives a water feature.

The independent `seed0-water-magma.json` fixture captures all **13,824 columns** around six
sites, including 760 water tops, 184 magma tops and 317 lava tops. Together the three
surface fixtures check **36,864 column observations**, with repeated sites explicitly
retained: 1,821 water, 644 magma and 1,229 lava observations. All special surfaces are checked
across eight map layers with colorblind/highlight and relief/contour settings. These counts
validate saved-data rendering, not procedural forecast accuracy. Normal cinder seed-only
results still depend on canonical decoration ordering and a solid-column approximation;
caves, later spring placement and fluid ticks are not fully simulated.

World-0 water examples: `1856,1147` (Y105), `14840,14327` (Y105), `-3191,5149` (Y98).
Exposed magma examples: `13652,14158` (Y98), `-14062,40758` (Y85), `-49485,92560` (Y75).
Open world `0` and zoom to block scale; detail mode can stay disabled.
