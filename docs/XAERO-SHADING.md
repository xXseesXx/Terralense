# Xaero terrain shading reference

The installed Xaero World Map 1.47.0 profile has `lighting=true`,
`terrain_depth=true`, and `terrain_slopes=2` (Default 3D). The numeric helper in
`src/hillshade.rs` ports the Overworld depth and 3D slope terms for these
settings. The installed jar is inspected locally; the reference program
`native/terralens-rs/tools/reference/XaeroSlopeReference.java` independently evaluates the
decompiled equation and is not bundled into runtime code.

Enable **Layers → Terrain shading** and set its strength to **100%** for the
full terrain equation. New preferences default to 100%; existing sessions keep
their selected strength. **Ground blocks**, **Grass colors** and **Trees** use
texture profiles instead of the categorical biome palette.

Xaero stores height differences from the previous north and north-west map
columns in signed bytes. Mode 2 forms a surface-facing cosine from those two
differences, rounds its positive direct-light term up to tenths, and combines it
with 0.2 colored ambient light and 0.5 white ambient light. The terrain-depth
term is `clamp(surface_y / 63, 0.9, 1.0)`. A caller may blend the resulting
multiplier with unshaded RGB using the TerraLens strength setting.

The Java reference sweeps 1,030,410 combinations across 10 heights and all
signed-byte slope values, including inputs beyond the byte clamp. Rust checks
the same output digest in `hillshade::tests`:

```text
samples=1030410 fnv1a=a50d47e4446be96a
```

Block colors use XaeroLib 1.7.3's square sampling grid, alpha cutoff and equal
RGB averaging, followed by the same float multiplication/truncation for tint.
The profile samples the installed mod/KubeJS assets and enabled file resource
packs: 201 ground blocks plus the neutral grass-top overlay, 106 tree texture
families and 173 forest/palm feature mappings. Full texture images and Xaero
code are not bundled. Resolved tree-color lookups allocate no per-pixel strings.

This covers only the configured terrain depth and geometric slope terms.
Xaero also shades each block from block and sky light, adds transparent water as
an overlay above the final terrain block, blends biome tint and texture colors,
and composites pixel overlays. Those inputs require source chunk light arrays
and block rendering state; TerraLens's seed sampler does not have them. Its
slopes at coarse zoom also use one sampled height per output pixel, while Xaero
generates block-level color before its own zoom compositing. Saved heights and
seed-only heights therefore have different confidence even when the equation
matches.

In particular, water keeps the viewer's opaque blue palette by default and is
shaded as a whole; Xaero shades the seabed and then composites lit, translucent
water over it. The simulated-date slider adds TFG monthly water and grass colors,
species autumn/winter foliage, AFC blossoms and palm lifecycle colors. Its
annual-average reset retains the earlier palette; neither setting reads the
world's live weather or calendar. The crown-edge fallback also
keeps the current canopy height at missing crowns rather than sampling a full
3D block model. These prevent claiming pixel-identical Xaero screenshots.

The final release suite passes 500 tests, including tile/export agreement and
shared tile-edge texels. The installed Java dormant-shield height oracle adds
128 multi-seed and piecewise-threshold checks. A 22-tile release-engine run
records cold block-view times of 0.40–0.47 seconds and a 2.81-second medium
regional tile with two engine threads; these exclude UI/GPU presentation and
are not frame-rate measurements. See [the recorded timings](data/xaero-render-performance.json).

The relevant installed source paths after local decompilation are
`xaero/map/region/MapPixel.java`, `xaero/map/region/MapBlock.java`,
`xaero/map/region/MapTileChunk.java`, and
`xaero/map/common/config/option/WorldMapProfiledConfigOptions.java`. User
settings are in Xaero's `config/xaero/world-map/client.cfg`.
