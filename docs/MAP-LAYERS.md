# Map layers and vegetation

The **Layers** menu is the only place to change map composition. Its active rows are
listed foreground to background; use ↑ and ↓ to change their order. Each active row
has an opacity slider, and each checkbox independently enables that layer. New layers
start at 55% opacity, except the first layer, which starts opaque. Grass colors and
trees are independent overlays in the same menu. Numeric shortcuts and display
presets add their layer without clearing the composition.

**Layers → Grass colors** adds the installed TFC climate-based grass colors. It replaces categorical
biome colors on grass-compatible terrain, and also works with no categorical layer
selected. Temperature, rainfall, elevation and other translucent layers can sit above
it. Open water and exposed magma keep their colors; canopies above those physical
surfaces render on top.

In the biome view, dry lake-biome columns and covered mountain rivers use a darker
blue to distinguish them from exposed water. They retain their biome identity;
the annotation does not flood the solid terrain or enable ground-material colors.
Ground blocks and explicitly enabled Grass colors still show the visible surface.

**Layers → Trees** adds projected trees independently of full terrain detail.
Complete saved chunks supply the actual exposed leaf and wood columns; missing chunks use
the installed tree templates and modeled placement rules. Seed-only positions
remain approximate where substrate, neighboring decorations or the runtime feature
order differ; see [vegetation generation](VEGETATION-GENERATION.md). The overlay is
visible closer than four blocks per pixel and skips decoration work at distant
scales. It renders last, above ground, geology, relief and contour lines, at the
selected tree opacity. Exposed branches and dead trees retain their wood silhouettes.
When relief is enabled, crown shading follows tree heights rather than buried terrain.

**Layers → Ground blocks** shows the exposed TFC ground material, including soil
types, sand variants, gravel and rock. It is an Overworld layer and can be composed
with the other layers. Open water, lava and magma retain their fluid colors. Saved
chunk blocks are used where available; unexplored terrain uses the installed TFG
surface rules. Saved material reads are limited to local scales (4 blocks/pixel
or closer); distant maps use the seed. Unexplored slope, glacial and decoration
surfaces remain approximate; see [Ground materials](GROUND-MATERIALS.md).
The layer is available as `--layer ground` for offline rendering and
as the `9` layer shortcut.

Grass and foliage use the original TFC 256×256 colormaps, discrete climate lookup
and TFG elevation adjustment. The compact slider in the top bar chooses a simulated
time of year; the sun button restores annual-average colors. Its date changes
grass, supported tree foliage, AFC blossoms, palm lifecycle colors and water using
the installed TFG rules. Ordinary conifers retain their green palette. The selected
date uses averaged monthly temperature without daily weather or hour-of-day noise;
it does not read or alter the world's calendar. Soil, bare rock and tree geometry
remain the same. Dates persist in sessions and exported maps use the same colors.
Grass colors and trees are Overworld options. Dimension changes retain their choices
and filter out map layers unsupported by the selected dimension.

Layer settings and opacities persist in preferences and sessions, and PNG exports
use the same composition as the viewer. The legend can inspect any active visible
layer without changing the composition. Rendering quality, color palette, filters,
relief, height lines, terrain detail and feature visibility are grouped under
**Map → Display options…** (also accessible from Tools). **Layers → Terrain shading**
also exposes the relief toggle and strength slider directly. Tooltip fields have their
own submenu. Map → Map guides contains coordinate, chunk and region grids, the
scale bar and connected-region highlight.

Terrain shading uses the installed Xaero World Map profile's 3D slope/depth
equations at 100% strength (the default for new preferences). Existing saved
strength values are retained. Ground and tree colors use Xaero's texture-sampling
grid and Minecraft's texture × tint multiplication. This matches those numeric
operations, while seasonal foliage, block lighting, translucent fluids and some
block-model details remain approximate. See [the rendering reference](XAERO-SHADING.md).

Opaque full-terrain images are displayed only on an unstyled biome map. Active relief,
height lines, palette changes, biome highlights, composed layers, grass colors and
trees keep raster rendering visible at close zoom. The terrain detail setting remains
off by default.

Rendering builds one shared sampling plan per tile/export, requests only fields
needed by its active layers, and reuses terrain/biome/fluid grids across styles.
Tree source results have a process-wide 32 MiB cache budget; target canopy chunks
are cached separately by seed, settings and save revision. Cancelled requests do
not publish partial canopy data.

## Terrain cost and accuracy

**Map → Terrain detail** selects Fast, Balanced, Full or individual generator
features. Fast is the default for newly created preferences: it renders the quart
biome classification and existing structure/ore markers, without terrain heights,
cave/carver columns, volcanic pools, springs, dikes, boulders, soil discs, or plant
sampling. Rivers/lakes are shown as biome categories in this mode; their exact
water plane and buried channels require the detailed surface options.

Balanced adds base surface materials, terrain relief, water and volcanic surfaces,
and large rocks/soil patches while omitting cave columns, intrusive dikes, springs,
small litter and tree canopies. Full selects block-precise boundaries, all terrain
features, grass, trees, relief, contours and surface litter at block maximum detail.
Each feature can be turned off independently. The physical-terrain checkbox enables
its subordinate feature controls; precise biome boundaries and litter are separate.
Previous preferences without a terrain field keep their prior full terrain behavior.

Choosing a preset leaves marker visibility/group filters intact. Complete structure
and ore scans are retained across quality, palette, layer and calendar changes.
Those generator results never use the visual terrain approximation to determine
where structures or ores exist. Explicit teleport copies still resolve their actual
destination height even in Fast mode. The basic hover skips terrain unless an
explicit height/ground field is enabled.

**Layers → Surface litter** shows loose/mossy rocks, sticks and twigs, fallen
leaves, dry grass, salt licks and recognized saved groundcover. It is independent
of grass colors, ground materials and tree canopies. Debris is painted over ground
before relief and tree crowns, and is hidden on open water and at sampling steps
coarser than four blocks per pixel. Saved chunks supply actual exposed debris;
unknown chunks use the source-based seed projection. See [surface litter](SURFACE-LITTER.md)
for supported generation and known state/order limits.
