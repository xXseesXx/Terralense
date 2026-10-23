# TerraLens notices

TerraLens is distributed under the European Union Public Licence, version 1.2. See [LICENSE](LICENSE).
Native Rust conversion, Prism import, renderer and viewer additions were made on 22–23 October 2026.
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

No mod JARs, player files, save chunks, account credentials or upstream Git history are distributed.
The fixture observations are immutable numeric/block-ID captures used for verification.

## Ore artwork

Unlike the original OutCrop distribution, TerraLens embeds small ore-marker PNGs from the installed
pack. TFC normal-grade ore, loose-rock and kaolin artwork comes from the TFC assets shipped with
TFG Core 0.9.25. GT-backed icons combine the GTCEu 7.5.3 raw-ore texture layers with the installed
material colors and TFG icon-set overrides. Those assets retain their respective upstream licenses;
TerraLens does not claim authorship of them. The exact material/layer provenance is in
[native/terralens-rs/assets/ore-icons/README.md](native/terralens-rs/assets/ore-icons/README.md)
and its `vein-materials.json`.

GTCEu credits Gregtech: Refreshed / ULSTICK, Ghostipedia, ZedTech contributors, TTFTCUTS, Rosethorns
and other contributors for its artwork; its full credited-works list is at
https://github.com/GregTechCEu/GregTech-Modern/blob/v7.5.3-1.20.1/README.md.
Copies of LGPL-3.0 and GPL-3.0 are in `licenses/`; the source projects remain available at the
links above. The structure marker glyphs and application icon are original TerraLens artwork.
No Cubiomes Viewer or Amidst textures were copied.

## Fonts and Rust dependencies

The embedded, unmodified Hack-Regular font used for PNG export text is from epaint_default_fonts
0.33.3 / Source Foundry's Hack project: https://github.com/source-foundry/Hack.
Its MIT and Bitstream Vera notices are retained in
[native/terralens-rs/assets/export-font-source.txt](native/terralens-rs/assets/export-font-source.txt).

Rust dependencies retain their own licenses. Exact versions and checksums are in `Cargo.lock`.
The application links eframe/egui, serde, flate2, rayon, crossbeam-channel, lru, image, md5 and ab_glyph.
Source and license information accompany those packages in the Cargo registry.

Minecraft and mod names are used to identify compatible data. TerraLens is independent of their
publishers and of Cubiomes Viewer and Amidst.
