# Bundled ore marker art

The PNGs are embedded in the Rust executable and uploaded to the GPU on demand. TerraLens does not scan the Prism instance or read texture files while running.

The source artwork comes from the installed `TerraFirmaGreg-Core-Modern-0.9.25.jar` (`assets/tfc/textures/item/ore`, item loose-rock/kaolin textures) and `gtceu-1.20.1-7.5.3.jar` (`assets/gtceu/textures/item/material_sets`). TFC materials use the installed normal-grade raw-ore item PNG where it exists. GT materials use the GTCEu raw-ore item texture layers selected by the actual TFG material icon-set override from `kubejs/startup_scripts/tfg/icon_sets.js`, resolved through the GTCEu icon-set parent chain. The fallback's layer-0 RGB and secondary RGB use the installed GTCEu `Material.Builder.color` and `secondaryColor` values recovered from its material definitions; the TFG-only Tarkianite value comes from `primitive/materials.primitive.js`. These are material colors applied to the original game texture layers, not hand-picked display swatches.

Dikes and gravel use their actual TFC loose-rock textures; kaolin uses the actual TFC clay item texture.

`vein-materials.json` lists the GT-backed vein icons with their material, TFG icon-set override, resolved texture family, and exact primary/secondary RGB values. Its Beneath entries use each configured vein's dominant weighted ore material. Their 16×16 icons are baked from the installed GTCEu raw-ore item layers using that material's installed colors and the terminal texture family reached through its TFG/GTCEu icon-set parent chain. The Beneath material labels and dominant-material selections are checked against the normalized installed vein configurations in Rust tests.

The [planetary catalog](planetary/README.md) covers every configured Moon and Mars vein. Its
manifest records the primary mineral, exact texture source and material colors; common minerals
share one texture in the viewer. Catalog tests verify all 55 types, their configured primary
outputs, transparent sprite silhouettes and position-independent lookup.
