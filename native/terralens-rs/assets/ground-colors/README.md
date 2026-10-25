# Ground palette

`palette.json` stores sampled RGB values for 202 matching TFC/TFG block textures in
the TerraFirmaGreg-Modern Prism profile. `tree_palette.json` stores texture samples
for 106 wood species and maps 173 installed forest/palm features to their real leaf
and trunk texture families using their NBT palettes. These are compact texture-color
profiles; full PNGs stay in the game installation.

The palette contains 201 block-color entries plus the neutral grass-top overlay. KubeJS assets override matching mod
textures; missing vanilla terrain colors come from the cached Minecraft client jar.
Hardened TFC rock shares its raw-rock texture. Sea ice uses the vanilla ice color.

Regenerate with `python3 extract_palette.py /path/to/instance/minecraft` followed by
`python3 ../../tools/reference/extract_tree_palette.py` (Python Pillow required for
the first command). This includes raw rock, gravel, sand, dirt, clay, mud, rooted dirt,
and tree texture paths found in installed mod jars, KubeJS assets and selected file
resource packs. `tfc:grass/<soil>` is
climate tinted by the app using the existing TFC grass colormap; its substrate texture
color is not used as a grass tint. Water and lava retain their dedicated map colors.

Farmland uses its top texture, paths use their `_top` texture, and raw sandstone
uses the `sandstone/bottom` texture referenced by the installed raw sandstone models.
Both ordinary and clay grass variants receive the climate tint.

The sampler matches the installed Xaero World Map 1.47 / XaeroLib 1.7.3 texture-color
routine: it samples a regular grid over the square area of `min(width, height)`, with
stride `max(1, min(4, side / 8))`; pixels with alpha at most 10 are ignored; sampled
RGB channels are averaged equally with integer truncation. It does not alpha-weight
partly transparent samples. For tree blocks, the map selects a model quad and its
texture; the bundled profiles use the TFC leaf sheet or log-end sheet as a species
proxy. Runtime block-state properties, alternate models, current resource-pack order,
and stateful biome tint can change the exact Xaero pixel.

TFC grass top faces use the neutral grayscale `tfc:block/grass_top` overlay multiplied
by the TFC grass color. Tree feature entries resolve NBT palette block IDs to actual
species textures; this covers the AFC wood aliases and TFG fruit-palm leaves as well
as the base TFC woods. TFC deciduous leaves tint by saved calendar season and species
autumn index, while conifer and other custom leaf providers have their own rules. The
seed viewer has no saved TFC calendar value for a seed-only map, so it uses the annual
foliage tint where a current date cannot be determined.

The extractor also reads vanilla terrain textures from Prism's cached Minecraft 1.20.1
client jar (when available), including sand, gravel, stone, tuff, snow and ice variants.
TFG duff variants use the shipped duff top-face texture. Grass blocks are climate tinted
from the TFC/TFG biome grass color map at render time.

Mossy cobblestone and muddy roots retain their own texture averages. Podzol column
blocks use the model's top-face texture rather than the side texture, including
coarse dirt, rooted dirt, clay, and mud variants. Internal `_top` texture names are
stored under the corresponding placeable block ID.

Source textures are distributed with the installed TFC/TFG mod jars and local KubeJS
assets; this generated palette contains only averaged RGB values. Paths use the source
namespace and texture-relative filename, for example `tfc:rock/raw/granite`.

`litter_palette.json` stores neutral RGB and alpha coverage for recognized
surface debris. Loose and mossy loose rocks fall back to raw/mossy-cobble model
textures; species twigs use the profiled log-end texture. Seasonal leaf-litter tint
is applied separately, and conifer litter has TFC’s fixed orange tint. Ground and
litter palettes remain separate. Refreshing the texture extractor preserves the
wrapped tree palette’s runtime feature aliases; `extract_tree_palette.py` refreshes
those aliases from installed NBT model data.
