# Structure layouts and container inspection

Pin a structure to load its geometry and containers in the background engine. The pinned panel
lists containers by their block coordinates. Select a row or click its chest glyph on the map to
inspect the inventory. **Cutaway** selects the highest visible Y; selecting a container moves
the cut just above it. **Show structure layout** controls the overlay independently of terrain
detail mode. **Fit layout** centers and zooms to the shape. Saved layouts have an explicit
**Refresh** action for changes made in Minecraft.

Saved inspection reads the structure start and individual piece bounds, then reads the generated
chunks intersecting those pieces. Shapes show the observed blocks inside the piece volumes,
including surrounding terrain within those volumes. Missing chunks remain missing and make the
inspection incomplete. Containers outside the individual pieces are excluded, even when inside
the overall structure rectangle. TFC vessels with nested saved inventories are supported.

Unexplored layouts use template geometry extracted from the installed pack at maintenance time.
The start-height provider, Java large-feature random stream, rotation draw and weighted template
choice determine the start piece. Rotation is around the Minecraft template origin, including
negative offsets. Projected terrain heights remain estimates. Layout notes identify incomplete
assembly and processors that can change blocks or inventories; an available template is not
proof that a candidate survives every Minecraft placement check.

Connected template forecasts follow rotated jigsaw connectors, weighted pools, fallback pools,
composed list elements and bounded limited delegates. These forecasts are explicitly labeled
**Partial template layout**. Pool limits, collision handling, terrain projection and structure
processors are not a complete Minecraft assembly port. In the two reference locations, the
current forecasts contain 79 and 16 pieces, whereas the saved village and Moon base contain 90
and 24. Those counts do not mean the individual pieces match, nor that every forecast container
will appear in-game. Saved inspection supplies the actual generated geometry and inventories.

The saved world-0 mud-brick ruin at chunk 1231, -204 selects `ruin_small/ruin_8` with no rotation;
chunk 1244, -222 selects `ruin_small/ruin_3` clockwise 90 degrees. Both start-template selections
and horizontal bounds were checked against the saved starts. The first ruin's two saved vessels
contain six straw plus six small sphalerite ore, and eighteen clay balls, respectively.

Additional [read-only observations](../native/terralens-rs/tests/fixtures/loot-sites/world0-lootr-observations.json)
cover an Arabic village with 90 saved pieces and a Moon base with 24. Native inspections
recovered the reference Lootr inventories in both dimensions: 21 and 18 saved item stacks,
respectively, with matching item totals. Moon Lootr records correctly resolve through the
world's shared overworld data directory while validating the Moon dimension stored in the record.

Inventory facts have separate meanings:

- Saved inventory items are observed contents, including known-empty inventories.
- Fixed template inventory items describe the selected template. Unsupported processors can
  prevent claiming those contents as exact.
- **Possible drops** enumerates resolved loot-table entries and entry stack ranges. It does not
  claim a particular roll, number of stacks, or guaranteed drop. Conditions and context-dependent
  modifiers are recorded in the container notes.
- Lootr inventories are saved separately for each player. The installed pack randomizes seeds
  when a player first opens a container. An unopened Lootr chest has no exact shared inventory
  derivable from world seed and position. See [Lootr behavior](LOOTR.md).

Inspection is on demand and cached by structure instance across pans and zooms. Chunk decoding,
resource assembly, loot lookup and IPC packing run outside the window process. Shape transport
uses a block palette with bounded binary records. Geometry is capped at 160,000 blocks and saved
reads at 1,024 nearest structure chunks; an exceeded limit is reported as incomplete. The display
uses a cached sparse raster, preserving one texel per block for footprints up to 1,024 blocks.
All save files are opened read-only. The runtime requires no Minecraft, JVM or Python process.
