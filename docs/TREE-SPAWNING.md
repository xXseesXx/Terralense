# Tree spawning

The independent tree overlay and terrain detail share forest feature attempts.
Complete saved chunks use their actual exposed tree blocks. Unexplored terrain
uses the installed TFG 0.9.25 / TFC 3.2.25 rules against the preview's terrain.

Tree eligibility is tested at the root, before roots, trunk and palette RNG are
consumed. A failed placement rejects the whole attempt; foliage is not culled
because the ground underneath a distant part of its crown is rock or water.

The sampler now checks:

- The actual merged `tfc:tree_grows_on` block tag. Ordinary trees accept tagged
  dirt, grass, mud and TFG soils, including duff, alfisol, mollisol, oxisol and
  podzol. Exposed raw rock, boulders, ash, gravel and ice do not qualify.
- The configured footprint after mirror and rotation: air at the root, valid
  support directly below it, and trunk clearance where the feature requires it.
  Configured trunk voxels and known earlier projected tree cells obstruct later
  attempts.
- Sand only for tree configurations that allow it. Water support follows the
  source's normal, shallow, submerged and floating distinctions, including the
  freshwater/saltwater and river restrictions represented by the terrain model.
- Natural features at the correct decoration stage. Earlier spring beds and
  boulders affect roots; later empty springs and soil discs do not. Forest height
  queries use first air above the solid cap, rather than confusing that value
  with the surface block's Y.
- Palm patches' separate bamboo-support tag and three-dimensional saltwater
  search box. Ocean biome proximity alone is insufficient, particularly on
  cliffs or dry ocean-biome terrain.

The block tags are extracted from the installed Minecraft client, mod resources
and KubeJS overrides by `tools/reference/extract_tree_ground.py`. `--check`
reproduces the embedded profile. Focused regressions preserve all TFG soil
families, distinguish ordinary roots from palms on sand, and check fluid support
and the palm search's vertical bounds.

## Remaining limits

The preview does not generate a complete 3D chunk. Its surface blocks, cave
clearance, collisions with neighboring decoration chunks and unprojected template
blocks remain approximate. Conditional ordinary-root weighted draws and replacement
writes are modeled, using estimated subsurface soil/rock profiles. A wrong soil
profile can still shift later random draws. Canonical offline
feature ordering is also distinct from a captured live JVM registry order.

Saved-world foliage comparisons count canopy columns, not individual tree
origins. Those references include bushes and other foliage; they cannot certify
a per-tree error rate. The existing six-site volcanic fixture measured:

| Canopy columns | Previous permissive gate | Current placement rules |
| --- | ---: | ---: |
| True positives | 2,369 | 1,820 |
| False positives | 2,289 | 740 |
| Missed saved foliage | 776 | 1,325 |
| Precision | 50.9% | 71.1% |
| F1 | 0.607 | 0.638 |

False-positive columns fell 67.7%; missed foliage also increased because terrain
estimates cannot always supply the real substrate or clearance. This biased
sample is a regression check, not a global accuracy claim. See
[the current report](data/tree-spawning-world0.json) and
[vegetation generation](VEGETATION-GENERATION.md) for the reference command.

The latest broad 48-chunk sample has canopy precision 52.5%, recall 65.5%, F1
0.583; the earlier spread baseline was F1 0.580. This different sample includes
29 biomes and all modded foliage. Forest origins read the retained WG heightmaps,
while support tests see earlier springs/boulders and placed tree occupancy.
New crowns do not raise the retained WG heightmap during decoration. See
[the spread accuracy report](GENERATOR-ACCURACY.md) for denominators and limits.
