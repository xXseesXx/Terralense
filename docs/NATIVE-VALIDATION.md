# Native validation

The published release was checked on Linux against TFG Core 0.9.25, TFC 3.2.25 and Beneath 1.0.6.
The desktop-shell refork passes **233 release tests**: 211 library, ten engine-process, ten parity
and two release-CLI checks. Formatting, the headless build and the opt-in diagnostic targets pass.
This includes 37 Atlas tests covering cache reuse across zoom levels, refinement, family coverage,
adjacent boundaries, split continuation, same-root panning and dimension-aware spawn/player pins.
Saved-location fixtures cover spawn, UUID deduplication, multiple players and dimensions, malformed
files, and both player-name cache formats. Defaults migrate once and preserve subsequent choices.

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

## Viewer behavior

- The refactored shell was rendered at 940×620 and 1280×800 on an isolated X11 virtual display,
  including the Beneath Y96–111 band. Ctrl+F focused the search field and accepted a query; the
  rail switched to raw ore artwork; Structures and editable analysis coordinates fit the dock.
  Earlier release checks covered result navigation, legends and saved block inspection in both
  dimensions. Heavy jobs remain in a separate process. These checks are not Wayland liveness proof.
- The 46px icon rail fits at 940×620, with structure/ore switches stacked above the type icons.
  Right-click expands grouped ores and structures, and child icons control individual variants.
  Catalog partition checks keep each vein selectable once, preserve dimensions and namespaces,
  and verify mixed-vein grouping against configured outputs. Spawn and player artwork stays
  separate from feature clusters and appears in composed PNG/CSV exports.
- Native icon rendering was checked at 940×620 with 32px transparent structure sprites and larger
  raw ore artwork. Clusters use representative artwork and counts without a disc background.
  Sprite tests cover alpha, the mineshaft opening, TFG symbol routing and distinct vanilla textures
  sharing a structure family. Export tests check transparent icon corners on the map image.
- Native asynchronous file picking was opened on the desktop. Portable source fixtures and a real
  engine child test check source import and failure replies; they do not simulate an OS chooser.
  File/folder world IDs agree, and same-named worlds from different locations remain distinct.
- Imports accept arbitrary world, saves and Prism folders. Failed session-source opens retain the
  current document; newer world/seed selections invalidate pending import replies. Structure CSV
  export snapshots the filtered table, and Save As changes the session destination after success.
- Structure reader fixtures cover arbitrary mod namespaces, dimension-root isolation, and boxes
  whose start point lies outside the visible map. The local reference save had bounds for all
  349 Overworld and 2,875 Beneath starts, with zero decode errors.
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
