# Notices and attribution

OutCrop is **Licensed under the EUPL, Version 1.2** (see `LICENSE`).

Copyright © 2026 Pau Aixalà.

This file records what this project takes from other people's work and under which licence, so that
anyone receiving a copy can see it without reading the source. It is the attribution half of what
those licences ask for; `LICENSE` is the other half.

Data files under `src/data/` that come from another project are written by scripts in `tools/` and
record in a `_meta`, `_source` or `source` field where they were read from. The colour palettes in
`src/data/palettes/` are this project's own design. The licences below are the ones each project
declares: its repository's `LICENSE`, or the `license` field of its jar's `META-INF/mods.toml`.

---

## 1. TerraFirmaCraft — EUPL-1.2

- Project: TerraFirmaCraft, <https://github.com/TerraFirmaCraft/TerraFirmaCraft>
- Licence: European Union Public Licence v. 1.2
- Branch used as reference: `1.20.x`, commit `b158c9c9`; the TerraFirmaGreg instance runs 3.2.24

**What this project takes.** The world generation is a **reimplementation in TypeScript** of
TerraFirmaCraft's own algorithms, written by reading its Java source: the region pipeline, climate
model, biome layers, rock layers, river generation, surface height and the ore vein features; and
the anvil mechanic the forge solves (`ForgeStep`, `AnvilRecipe`). Every port cites the Java class
and method it came from in a comment above it.

No TerraFirmaCraft source file is copied into this repository. The algorithms are nonetheless
derived from EUPL-licensed work, which is why this project is EUPL-1.2 and not something more
permissive.

**Extracted data.** `src/data/tfc-1.20/` contains values regenerated from TerraFirmaCraft's own
datapack and source by `tools/extract-datapack.mjs`: ore vein definitions, rock and biome lists,
biome tags, anvil recipes, anvil action deltas and crop/plant climate ranges; by
`tools/extract-biome-palette.mjs`, the biome colours of its `RegionGeneratorTest`; and, by
`tools/extract-rock-categories.mjs`, each rock's display category from its `Rock` enum
(`rock-categories.json`, which the rock guide shows). The 486 `tfc:` recipes in `src/data/tfg/anvil-recipes.json` are
TerraFirmaCraft's recipes as the TerraFirmaGreg game holds them, some replaced under the same id by
TerraFirmaGreg's scripts (section 2). `src/data/tfg/` holds three more pieces of it: the same
`ForgeStep` action deltas (`anvil.json`); the rock layers of its `tfc:overworld` world preset, in
the order the saved world's `level.dat` holds them (`rock-layers.json`); and five vein definitions
that TerraFirmaGreg adds to its overworld by tag (`granite_dike`, `diorite_dike`, `gabbro_dike`,
`gravel` and `kaolin_disc` in `veins.json`). This is upstream data, redistributed here under the
same EUPL-1.2.

## 2. TerraFirmaGreg (Modpack-Modern) — LGPL-3.0

- Project: TerraFirmaGreg Modpack-Modern, <https://github.com/TerraFirmaGreg-Team/Modpack-Modern>
- Licence: GNU Lesser General Public License v3.0
- Revisions read: commit `9a8c6f34` (vein tables) and `f456f468` (rock table), and the KubeJS data
  and scripts of the installed modpack, version 0.13.8 (which ships TerraFirmaGreg-Core 0.9.21,
  section 3)

**What this project takes.** Values regenerated from the modpack's KubeJS data and scripts, into
`src/data/tfg/`: the vein definitions of every dimension, the rock table, the Beneath's rock
layers, kaolin clay placement, the climate routers of the Beneath, Moon, Mars, Venus and Glacio
(`dimensions/*.json`), and the structure sets of `structures.json` with their placements, biome
lists and spawn conditions. The 45 `tfg:` recipes in `src/data/tfg/anvil-recipes.json` are the
anvil recipes its KubeJS scripts write, as are the replacements it makes under `tfc:` ids. The
Beneath's rock model (`src/worldgen/tfg/beneath.ts`) is a reimplementation of its
`server_scripts/tfg/worldgen/events.chunks.js`.

No TerraFirmaGreg source file or mod asset is copied into this repository.

## 3. TerraFirmaGreg-Core (Core-Modern) — LGPL-3.0, with a caveat

- Project: TerraFirmaGreg-Core, <https://github.com/TerraFirmaGreg-Team/Core-Modern>
  (<https://www.curseforge.com/minecraft/mc-mods/terrafirmagreg-core>)
- Revision: tag `0.9.21`, commit `2cf74e65`; jar `TerraFirmaGreg-Core-Modern-0.9.21.jar`
- Licence: **the two sources disagree, and this project follows the repository.** The repository
  ships the GNU Lesser General Public License v3.0 as its `LICENSE` at commit `2cf74e65`. The
  released jar's `META-INF/mods.toml` says `license = "All-Rights-Reserved"`, with the Forge MDK
  template comment still above it. Decided 2026-09-25: the repository's LGPL-3.0 governs. The
  namespaces it registers (`tfg`, `tfc_textile`, `tfcambiental`) are kept on that basis in
  `tools/public-namespaces.json`.

**What this project takes.** The `tfg` profile's world generation is a reimplementation in
TypeScript of Core-Modern's Java at `2cf74e65`, itself derived from TerraFirmaCraft: its regional
pipeline and initialisation (`TFGInitTask`, `TFGAddContinents`, the region mixins), biome layers and
biome noise (`TFGLayers`, `TFGBiomes`, `TFGBiomeNoise`, `TFGCellular2D`), rivers, climate and surface
height (`src/worldgen/tfg/`). Data regenerated from it: `src/data/tfg/biome-tags.json` (from the
0.9.21 jar), `biome-heights.json` and `biome-rivers.json` (from `TFGBiomes.java` at `2cf74e65`),
the biome ids in `biomes.json`, and part of `kaolin.json`. The structure extractor also reads the
Core jar, but every value in `structures.json` comes from sources it reads first; the Core jar
supplies none of them.

## 4. Mods whose anvil recipes the forge keeps

`src/data/tfg/anvil-recipes.json` is built from the `tfc:anvil` recipes the running TerraFirmaGreg
game holds (`tools/build-anvil-catalogue.mjs`). From each recipe it takes the input, result, anvil
tier and finishing rules, under the recipe's own id. A recipe is kept only if the namespace of its
id is on the keep list in `tools/public-namespaces.json`; `_meta.filtered` records what was left
out (at this revision, the 3 recipes `createdeco` writes, because Create Deco declares
`license = "Insert License Here"`).

| Namespace | Mod | Licence declared | Recipes |
| --- | --- | --- | --- |
| `tfc` | TerraFirmaCraft, section 1 | EUPL-1.2 | 486 |
| `tfg` | TerraFirmaGreg's KubeJS scripts, section 2 | LGPL-3.0 | 45 |
| `tfchotornot` | TFC Hot or Not, <https://www.curseforge.com/minecraft/mc-mods/tfc-hot-or-not> | BSD 3 | 18 |
| `rnr` | Roads and Roofs TFC, <https://www.curseforge.com/minecraft/mc-mods/roads-and-roofs-tfc> | MIT | 9 |
| `sns` | Sacks 'N Such, <https://www.curseforge.com/minecraft/mc-mods/sacks-n-such> | MIT | 6 |
| `firmaciv` | Firma: Civilization, <https://www.curseforge.com/minecraft/mc-mods/firmaciv> | MIT | 5 |
| `firmalife` | FirmaLife, <https://www.curseforge.com/minecraft/mc-mods/firmalife> | MIT | 3 |
| `waterflasks` | TFC Water Flasks, <https://www.curseforge.com/minecraft/mc-mods/water-flasks> | GPL3 | 2 |
| `afc` | ArborFirmaCraft, <https://www.curseforge.com/minecraft/mc-mods/arborfirmacraft> | MIT | 1 |

A recipe's result can be another mod's item (GregTech CEu, Create, Create Deco, Vintage
Improvements and others): the catalogue names that item by its id and takes nothing else from it.
The `tfc-1.20` catalogue, `src/data/tfc-1.20/anvil-recipes.json`, holds only TerraFirmaCraft's own.

## 5. Structure mods

`src/data/tfg/structures.json` lists the structure sets of the TerraFirmaGreg overworld. For the two
ruin mods below, the set placements, biome lists and spawn conditions come from TerraFirmaGreg's
KubeJS data (section 2), which overrides the mods' own sets; the biome tags in their jars add no
biome the TerraFirmaGreg overworld generates.

- **TFC Ruins** (`tfc_ruins`, by NoCube), <https://www.curseforge.com/minecraft/mc-mods/tfc-ruins>.
  Its jar declares `license = "Not specified"`. Taken from the jar: the structure type
  (`minecraft:jigsaw`) of its three structures, `ruin_small`, `ruin_beach` and `ruin_rich`.
- **TFC ruined world** (`tfcstructuremodc`, namespace `tfc_ruined_world`, by C4t2),
  <https://www.curseforge.com/minecraft/mc-mods/tfc-ruined-world>. Its jar declares
  `license = "All Rights Reserved"`, under the Forge MDK template comment. Taken from the jar: the
  structure type (`minecraft:jigsaw`) of its 16 structures.

## 6. Ad Astra — Terrarium License

- Project: Ad Astra, by Alex Nijjar, <https://modrinth.com/mod/ad-astra>
- Licence declared: `license = "Terrarium License"` (ad_astra-forge-1.20.1-1.15.20.jar)

**What this project takes.** Names only. The four planets are Ad Astra's dimensions, so
`src/data/tfg/dimensions/` carries their ids (`ad_astra:moon`, `ad_astra:mars`, `ad_astra:venus`,
`ad_astra:glacio`), and TerraFirmaGreg's vein tables for them name Ad Astra's stones
(`ad_astra:moon_stone` and the like) as the rock a vein replaces. Their routers and biomes come
from TerraFirmaGreg (section 2) and Minecraft (section 7).

No Ad Astra data ships. The Moon's climate router would carry one Ad Astra density function,
`ad_astra:depth`, but every Moon biome claims the same depth, so that entry cannot change a biome:
`tools/extract-dimension-climate.mjs` writes it as 0, and `tests/unit/dimension-climate.test.ts`
fails if an Ad Astra function or noise comes back.

## 7. Minecraft — Mojang

Minecraft is a trademark of Mojang Synergies AB. This project is not affiliated with, endorsed by or
associated with Mojang, Microsoft, the TerraFirmaCraft team or the TerraFirmaGreg team. No Minecraft
or mod assets — textures, models, sounds — are included. Every icon, glyph and texture in `public/`
and `src/ui/icons/` is original artwork made for this project.

**Reimplemented algorithms.** TerraFirmaCraft builds on Minecraft's own world generation, so parts
of it are reimplemented here from reading it, with no source copied: the random sources
(`XoroshiroRandomSource`, `LegacyRandomSource`, `RandomSupport` seeding), seed parsing, the noises
(`ImprovedNoise`, `PerlinNoise`, `SimplexNoise`, `NormalNoise`), the density functions and the
multi-noise climate search that place biomes in the other dimensions (`src/core/`,
`src/worldgen/vanilla/`), and random-spread structure placement.

**Values from the 1.20.1 client jar**, used where TerraFirmaGreg's data refers to them: two
structure-set placements in `src/data/tfg/structures.json` (`minecraft:strongholds`, concentric
rings; `minecraft:nether_complexes`, random spread with spacing 27, separation 4 and salt
30084232), carried only because other sets name them as exclusion zones; and the noise parameters
(`minecraft:offset`, `minecraft:continentalness`, `minecraft:erosion`, `minecraft:ridge`,
`minecraft:vegetation`) and density functions (`minecraft:shift_x`, `minecraft:shift_z`,
`overworld/continents`, `overworld/ridges`) that the Beneath, Moon, Mars and Venus routers in
`src/data/tfg/dimensions/` reference. Each file names its own in `noises` and
`_meta.density_functions`.

## 8. FastNoiseLite — MIT

- Project: FastNoiseLite 1.0.1, by Jordan Peck and contributors,
  <https://github.com/Auburn/FastNoiseLite>
- Licence: MIT

**What this project takes.** `src/worldgen/tfc-1.20/noise/fast-noise-lite.ts` reimplements the slice
of FastNoiseLite that TerraFirmaCraft vendors and configures (2D OpenSimplex2S, its hash and the
cellular jitter), including its two gradient tables, `Gradients2D` and `RandVecs2D`. Its notice, as
it appears in the copy TerraFirmaCraft vendors:

```text
MIT License

Copyright(c) 2020 Jordan Peck (jordan.me2@gmail.com)
Copyright(c) 2020 Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files(the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and / or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions :

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## 9. Silkscreen — SIL Open Font License 1.1

- Font: Silkscreen. Copyright 2001 The Silkscreen Project Authors,
  <https://github.com/googlefonts/silkscreen>
- Licence: SIL Open Font License, Version 1.1
- Licence text: `src/ui/fonts/OFL.txt`, vendored alongside the font files

Vendored as `@font-face` woff2 in `src/ui/fonts/` rather than loaded from a font CDN, because the
app must work offline. The OFL permits bundling and redistribution provided the licence travels with the font,
the font is not sold on its own, and a modified version is renamed — none of which this project
does. The files are the unmodified upstream subsets.

## 10. Smaller references

Written from their published specifications, with no code copied: `java.util.Random`'s linear
congruential generator and Gaussian (as its documentation specifies them), MD5 (RFC 1321), SHA-256
(FIPS 180-4), and one formula from fastutil (Apache-2.0), `HashCommon.long2int`,
`(int)(l ^ (l >>> 32))`.

---

## What the licences require of anyone distributing this

Both EUPL-1.2 and LGPL-3.0 are copyleft. In practice, if you publish this project — as a website or a
modified fork — you must:

1. **Keep the licence notice.** Ship `LICENSE` and this file, and do not relabel the project under a
   more permissive licence.
2. **Give people the source.** EUPL-1.2 §3 requires that the Source Code be made accessible to
   anyone who receives the work. A link to the public repository in the store listing or the site
   footer satisfies this, as long as the published version's source is actually there.
3. **Say what you changed**, if you distribute a modified version.
4. **Pass the same terms on.** Anyone you distribute to gets the same rights you did.
5. **Keep the attribution above intact**, including the trademark note and the FastNoiseLite
   notice.

The EUPL is deliberately compatible with the GPL family; §5 and its Appendix list the licences a
combined work may be relicensed under, which is what makes shipping EUPL-derived code alongside
LGPL-3.0-derived data workable.

This is a plain-language summary for orientation, not legal advice. `LICENSE` is the binding text.
