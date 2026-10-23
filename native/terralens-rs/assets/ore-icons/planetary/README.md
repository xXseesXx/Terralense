# Planetary vein icon assets

This directory contains one static 16×16 icon for each distinct dominant ore material used by the installed Moon and Mars vein definitions. `manifest.json` maps all 55 configured veins (27 Moon, 28 Mars) to their dominant material and records each material's installed colors, effective icon-set model, and texture source paths.

The dominant material is selected by summing the configured output weights across all host-rock replacement groups after stripping the installed host-rock prefixes. Raw block outputs are excluded. If multiple normalized ore materials have the same summed weight, the lexicographically first material ID wins. The per-vein weight is recorded in the manifest.

Colors come from the `Material.Builder.color` and `secondaryColor` values in the installed GTCEu 7.5.3 material definitions. Icon-set overrides and custom parent relationships come from the installed TFG `kubejs/startup_scripts/tfg/icon_sets.js`. Where the installed resource pack supplies a direct raw-ore item model for that icon set, its listed texture layers are used; otherwise resolution follows the icon-set parent chain to the first installed model. KubeJS texture overrides take priority over GTCEu jar textures.

The icon renderer applies the material primary color to layer 0, the secondary color to layer 1, and leaves later overlay layers untinted, then composites the layers in item-model order. These PNGs are generated assets; no Prism files are read at application runtime.
