# TerraLens notices

TerraLens's original source is under the European Union Public Licence, version 1.2; see
[LICENSE](LICENSE). Builds containing the Cubiomes Viewer icon assets are distributed as a combined
work under [GPL-3.0](licenses/GPL-3.0.txt), using EUPL-1.2 article 5 and its compatible-license
appendix. The original source retains its EUPL notices. The full corresponding source, original
art files, and build scripts are available at https://github.com/xXseesXx/Terralense.
Native Rust conversion, Prism import, renderer and viewer additions were made on 22–25 October 2026.
The changes include isolated generation workers, dimension-aware save reading, search, cutaways,
vegetation preview, map tools, original structure icons and export composition.

## OutCrop and generation references

The generator is derived from **OutCrop**, copyright © 2026 **Pau Aixalà**, licensed EUPL-1.2:
https://github.com/PauAixala/outcropmap

The reference checkout was pinned at `7d5b7940d96cbe134001416b09ea13862dba2598`.
The complete original attribution is retained in [licenses/OUTCROP-NOTICE.md](licenses/OUTCROP-NOTICE.md).
Its paths describe that upstream project, not this repository's directory layout.
The required tables were vendored into `native/terralens-rs/assets/data/tfg/`, and the two JVM RNG
captures into `tests/fixtures/core/`. Their provenance is retained. This Rust source and the
committed data are sufficient to build TerraLens; an upstream checkout is unnecessary.

The native algorithms and normalized configuration data also reference the installed TFC 3.2.25,
TFG Core 0.9.25, Beneath 1.0.6 and ArborFirmaCraft 1.0.22 generation rules. The relevant source
projects and their notices include:

- TerraFirmaCraft: https://github.com/TerraFirmaCraft/TerraFirmaCraft (EUPL-1.2).
- TerraFirmaGreg Core: https://github.com/TerraFirmaGreg-Team/Core-Modern (LGPL-3.0).
- TerraFirmaGreg pack: https://github.com/TerraFirmaGreg-Team/Modpack-Modern (LGPL-3.0).
- GregTech CEu Modern: https://github.com/GregTechCEu/GregTech-Modern (LGPL-3.0).
- ArborFirmaCraft 1.0.22: https://github.com/Therighthon/ArborFirmaCraft (the installed package declares MIT; Therighthon, with assets credited to Dodo). Its original MIT copyright and permission notice is retained in [licenses/ARBORFIRMACRAFT-MIT.txt](licenses/ARBORFIRMACRAFT-MIT.txt), copied from the published upstream history at `af8a3369c2a956fd45e43c03e961e436fcfbb8be`. Newer upstream revisions changed their license; the bundled blossom palettes retain the installed version’s attribution.

No mod JARs, player files, account credentials or upstream Git history are distributed.
The fixture observations are immutable numeric/block-ID captures used for verification.
Ground-material tests include stripped block-state sections from eight seed-0 chunks;
entities, containers, inventories and other save metadata are excluded.
The small ground-color palette contains texture-average RGB values; its source and
regeneration procedure are recorded in `native/terralens-rs/assets/ground-colors/README.md`.
Surface-decoration code follows TFC's `BouldersFeature`, `BabyBoulderFeature`,
`Metaballs3D` and `FlatEnoughPlacement`, and TFG's `SoilDiscFeature`. Effective
feature configurations are stored in `src/ground/decorations/assets/` within the
Rust crate; soil extraction follows the installed placed-feature references.
Natural springs follow TFC's `HotSpringFeature` and `Metaballs2D`, plus the
installed TFG/GTCEu bedrock-fluid selection and placement rules. Their source
notes and independent Java verification inputs are under `src/hot_springs/`.

The surface cave/aquifer pass follows TFC's `TFCAquifer`, interpolated density
functions and cave/canyon carvers, with TFG's three-dimensional river walls and
badlands sandstone rules. The OpenSimplex2S kernel and 3D vector tables derive
from FastNoiseLite 1.0.1, copyright © 2020 Jordan Peck and contributors, under
[MIT](licenses/FASTNOISELITE-MIT.txt). Extracted settings and source fingerprints
are retained in `src/caves/assets/` within the Rust crate.

Moon/Mars profiles also use Ad Astra 1.15.21 data, copyright Alex Nijjar and contributors:
https://github.com/terrarium-earth/Ad-Astra/tree/1.20.1 . Under the Terrarium License v1, files in
`resources/data` are code covered by its MIT permission. A copy is retained in
[licenses/AD-ASTRA-TERRARIUM.txt](licenses/AD-ASTRA-TERRARIUM.txt). Planetary configuration origins
and hashes are recorded under `native/terralens-rs/src/planetary/{assets,feature-assets,crater-assets}`.
No Ad Astra artwork is distributed in the source tree. Personal builds can embed the user's
local cheese item texture for the Moon cheese marker; that optional file stays outside Git.
The Moon pressure dome, rabbit nest box and source-only cheese fallback are original TerraLens
pixel-art code. Minecraft 1.20.1 registry parameter data and
small numeric oracle captures are used to validate the independently written Rust samplers.

## Ore artwork

Unlike the original OutCrop distribution, TerraLens embeds small ore-marker PNGs from the installed
pack. TFC normal-grade ore, loose-rock and kaolin artwork comes from the TFC assets shipped with
TFG Core 0.9.25. GT-backed icons combine the GTCEu 7.5.3 raw-ore texture layers with the installed
material colors and TFG icon-set overrides. Those assets retain their respective upstream licenses;
TerraLens does not claim authorship of them. The exact material/layer provenance is in
[native/terralens-rs/assets/ore-icons/README.md](native/terralens-rs/assets/ore-icons/README.md)
and its `vein-materials.json`.
Planetary icons also resolve the installed KubeJS item models and texture overrides before falling
back to GTCEu assets; their per-material sources are recorded in `ore-icons/planetary/manifest.json`.

GTCEu credits Gregtech: Refreshed / ULSTICK, Ghostipedia, ZedTech contributors, TTFTCUTS, Rosethorns
and other contributors for its artwork; its full credited-works list is at
https://github.com/GregTechCEu/GregTech-Modern/blob/v7.5.3-1.20.1/README.md.
Copies of LGPL-3.0 and GPL-3.0 are in `licenses/`; the source projects remain available at the
links above. The application icon and TFG-specific landmark sprites are original TerraLens artwork.

## TFC climate colormaps

The unchanged grass, water, default foliage, autumn and winter colormap PNGs come from TerraFirmaCraft 3.2.25,
distributed with TerraFirmaGreg Core 0.9.25, and are licensed under EUPL-1.2. Their exact
source paths and SHA-256 checksums are in
[native/terralens-rs/assets/map-colors/README.md](native/terralens-rs/assets/map-colors/README.md).
The climate sampler follows the installed `TFCColors.getClimateColor` indexing and TFG's
average-temperature elevation adjustment. Optional calendar simulation uses TFG's monthly
temperature rule, species foliage transitions and palm lifecycle colors. Four unchanged blossom
palettes come from ArborFirmaCraft 1.0.22 under MIT; its authorship and license metadata are
recorded in the asset README. Daily weather and time-of-day temperature variation are omitted.

## Cubiomes Viewer structure artwork

The vanilla structure and world-spawn PNGs are copied from **Cubiomes Viewer**, copyright Cubitect
and contributors, at commit `3acc863245b30c655498d60323c50da0865e0199`, under GPL-3.0. Its README
credits Amidst for biome-color and icon inspiration. The image files are unmodified; TerraLens's
renderer scales them and omits any solid black rectangular frame. The custom TFG firepit, village
head, cracked limestone masonry, cross and mineshaft arch were added on 23 October 2026.

The original sources, exact asset paths, file checksums, rendering changes and retained licensing
are documented in [the icon notice](native/terralens-rs/assets/structure-icons/README.md) and its
`cubiomes/provenance.json`. Upstream source and license:
https://github.com/Cubitect/cubiomes-viewer/tree/3acc863245b30c655498d60323c50da0865e0199

These works are provided without warranty. Recipients may copy, modify and redistribute the
combined application under GPL-3.0; the complete license is in `licenses/GPL-3.0.txt`.
The EUPL compatibility mechanism is described by the European Commission at
https://interoperable-europe.ec.europa.eu/collection/eupl/how-use-eupl.

## Fonts and Rust dependencies

TerraLens's terrain slope/depth equations and texture-sampling behavior were
independently implemented against the locally installed Xaero World Map 1.47.0
and XaeroLib 1.7.3 profile. No Xaero code or texture assets are distributed.
The numeric contract, independent reference checks and remaining differences
are documented in [the rendering reference](docs/XAERO-SHADING.md).

The embedded, unmodified Hack-Regular font used for PNG export text is from epaint_default_fonts
0.33.3 / Source Foundry's Hack project: https://github.com/source-foundry/Hack.
Its MIT and Bitstream Vera notices are retained in
[native/terralens-rs/assets/export-font-source.txt](native/terralens-rs/assets/export-font-source.txt).

Rust dependencies retain their own licenses. Exact versions and checksums are in `Cargo.lock`.
The application links eframe/egui, serde, flate2, rayon, crossbeam-channel, lru, image, md5 and ab_glyph.
Release archives bundle their license and copyright texts under `third-party-licenses/`, with a `THIRD-PARTY-NOTICES.md` index. Notices omitted from published crate archives are retained from their exact upstream commits in [licenses/rust/](licenses/rust/). The matching source archive includes vendored Cargo dependencies for offline builds.

Minecraft and mod names are used to identify compatible data. TerraLens is independent of their
publishers and of Cubiomes Viewer and Amidst.
