# Native validation

The published release was checked on Linux against TFG Core 0.9.25, TFC 3.2.25 and Beneath 1.0.6.
The production source export passes **198 tests**: 177 library, nine engine-process, ten parity
and two release-CLI checks. Formatting, the headless build and the opt-in diagnostic targets pass.

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

- Native window checks covered 940×620 controls, Ctrl+F result navigation, legends and saved block
  inspection in both supported dimensions. Heavy jobs remain in a separate process.
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
