# Native validation

The published release was checked on Linux against TFG Core 0.9.25, TFC 3.2.25, Beneath 1.0.6
and Ad Astra 1.15.21. The current release passes **289 release tests**: 265 library, twelve engine-process, ten parity
and two release-CLI checks. Formatting, the headless build and the opt-in diagnostic targets pass.
This includes 40 Atlas tests covering cache reuse across zoom levels, refinement, family coverage,
adjacent boundaries, split continuation, same-root panning and dimension-aware spawn/player pins.
Saved-location fixtures cover spawn, UUID deduplication, multiple players and dimensions, malformed
files, and both player-name cache formats. Structure regressions cover off-anchor box centers,
negative/inclusive coordinates, start-based identity and deduplication, viewport queries,
close-zoom clustering, search radius/ranking, and centered PNG/CSV/Xaero export destinations. Defaults migrate once and preserve subsequent choices.

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
The remaining per-dimension entries have no literal biome intersection in that pinned snapshot.
They remain in dimension-appropriate controls: a failed sample or incomplete native sampler
cannot blacklist a registered structure. Source checks still constrain preview positions and
only the explicitly empty Beneath outpost selector is treated as a pack disable. Actual saved
starts remain visible independently. A separate read-only check across four Overworld saves
found 1,048 recorded starts in 453,668 chunks and no excluded recorded registry IDs.

Portable release tests verify full audit coverage, current raw biome pools, separate preview
support and catalog availability, dimension-specific controls, retained unverified village
entries, and all 29 independently recorded saved types in the pinned audit.
The opt-in diagnostics regression additionally checks that all 37 reported witness locations
are returned by the production dimension scanners. Audit and finder use the same biome probes,
including Beneath at Y=93, and preserve all children in the weighted draw.

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
- Ruined churches, towers, towerhouses, castles, ancient monuments, aqueducts, ocean moai and
  other ruined sites share one expandable Ruins row across mod namespaces. Installed-catalog tests verify the consolidation,
  retained individual IDs and singleton solo behavior. A shared classifier assigns cracked masonry,
  broken chapel, shattered keep and standing-stone sprites in the rail, map and exports.
- Beneath structure pins ignore the selected vertical band in both the map and visible-marker
  exports, while ore pins keep their height filter. A cache regression checks that completed
  seed-placement pins survive repeated band changes and are cleared on a dimension change.
  Requested quiet defaults hide Ruins, Nether fossils, forest roaming, Moon cheese and rabbit
  houses. Migration/serialization tests verify one-time application and subsequent individual
  overrides, including ocean moai and aqueducts in the shared Ruins group.
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
