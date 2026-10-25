# Simulated calendar colors

The slider at the right of the top menu bar selects a simulated date from January
through December. The adjacent sun button restores **Average**, the previous
annual-mean appearance. This changes map colors; it neither reads nor changes the
world's clock. Grass colors, Ground blocks and Trees retain their independent
layer controls. Actual exposed water also follows the selected date.

The date persists in preferences and sessions and applies to image exports.
Overworld only: Moon, Mars and Beneath keep their existing colors. Snow and ice
are not simulated.

## Source rules

The Rust sampler uses the installed Core 0.9.25 / TFC 3.2.25 / AFC 1.0.22 rules:

- Monthly temperature uses TFG's latitude triangle, the world's temperature
  scale and elevation, with daily weather/hour variation set to zero.
- Grass and ordinary green foliage use that averaged monthly temperature and
  the original 256×256 climate colormaps.
- Deciduous foliage uses TFC's temperature-dependent spring/autumn boundaries,
  species autumn palette index and fixed positional variation. Ordinary conifer
  leaves use the green colormap; their krummholz forms follow their separate
  registered seasonal rule.
- AFC tree species use their installed indices and conifer flags. Its four
  flowering trees use the yellow, jacaranda, orange and red blossom maps with
  the source's month-transition hash.
- TFG palms use their species lifecycle and temperature/hydration range rather
  than the generic autumn rule.
- Water uses TFC's water colormap with the source resolver's fixed Y63 climate.

Saved species labels, registry feature IDs and palm age variants resolve to the
same color rules without per-pixel string allocations. Wood, exposed stone,
non-grass soil and crop geometry are not seasonally recolored. Existing palette
approximations for other plants remain outside this change.

## Caching and verification

Display changes retain the existing 180 ms debounce. Date changes invalidate
colored raster tiles while retaining completed marker scans, generator state,
height/biome/fluid grids and physical tree caches. Detail surfaces carry a compact
annual-climate array; their existing blocks and elevations are retained while
their GPU textures are recolored. No terrain generation or save scanning runs in
the slider's UI handler.

The numeric sampler matches **385 Java RGB controls**, including both hemispheres,
custom temperature scales, deep cave floors, species, palms, blossoms and exact
autumn boundary values at adjacent float representations. This oracle is a
standalone Java transcription of pinned source formulas using the original PNGs;
it is not a running Forge-client screenshot comparison. Source/artifact hashes
are embedded in `tests/fixtures/season-colors-java.json`.

Isolating the calendar change, all 22 previous normal-engine tile payloads were
byte-identical in Average mode; subsequent rock corrections have their own
separate validation.
Six normal-engine map exports exercise Average, January, April, July, October and
Average reset; the reset PNG is byte-identical to the original, while physical
surface probes are unchanged throughout. A 64×64 saved forest export took
0.233 s initially and 0.004–0.069 s on date changes in the final run; this is
generation/IPC/export timing, not UI or GPU frame timing. Reports:
[default parity](data/season-default-engine-parity.json),
[seasonal exports](data/season-engine-exports.json).

Regenerate numeric controls with:

```sh
cd native/terralens-rs
python tools/reference/season_colors_oracle.py --source /path/to/Core-Modern \
  --afc-jar /path/to/afc-1.0.22-1.20.1.jar --output /tmp/season-colors-java.json
```

# Teleport heights

Map context-menu teleport commands use the exact destination's resolved surface
block plus one for player feet. Saved ground and physical fluid surfaces take
precedence over raw terrain-noise height, so dry river banks, elevated water and
cave openings use the resolved column. Negative coordinates are floored to the
same block the sampler queried. A stale nearby hover probe cannot supply the Y.

Opening a map or marker menu requests the destination asynchronously, including
planetary height when its tooltip is disabled. If the result is not available,
teleport commands retain relative Y until a matching probe arrives. Explicit
marker elevations are preserved, and Beneath uses its selected Y slice. Copied
Xaero waypoints share the resolved-height helper; their format still requires an
integer Y, so unknown destinations retain a dimension default.
