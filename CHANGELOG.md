# Changelog

## 0.4.0

This release focuses on terrain detail, structure inspection and predictable interactive performance.

### Added

- Fast, Balanced and Full terrain profiles, independently configurable map layers, improved tile reuse, terrain shading and contours.
- Surface water and river rendering, physical ground materials, volcanic terrain and lava, hot springs, caves, tree overlays and optional surface litter.
- Seasonal color simulation and teleport-command output with modeled surface elevation.
- Structure previews with recorded bounds, container locations, saved chest/vessel contents and possible Lootr drops.
- Search across ores, structures, volcanoes, biomes, rocks and tree habitats; dimension-specific marker controls and improved saved-player markers.

### Accuracy and scope

- The seeded terrain model is not a complete Minecraft chunk generator. Results vary by feature and dimension; measured evidence and denominators are in [the accuracy report](docs/GENERATOR-ACCURACY.md) and [native validation](docs/NATIVE-VALIDATION.md).
- Structure candidate markers indicate placement candidates. Layouts and bounds are shown when recorded or available; unknown jigsaw layouts are not fabricated.
- Lootr chests can roll per-player on first opening. For unopened containers, TerraLens reports possible loot rather than an exact player-specific result.
- Full terrain detail, trees and surface litter are optional and more expensive than Fast mode. Seed-only vegetation and litter remain approximate; saved chunks provide stronger evidence.

### Compatibility

Release targets are Linux x86_64 and Windows x86_64. The validated pack profile is Minecraft 1.20.1 / Forge 47.4.13, TFG Core 0.9.25, TFC 3.2.25, Beneath 1.0.6 and Ad Astra 1.15.21. Other versions and custom generation profiles need separate validation. Platform test scope and build requirements are documented in [release preparation](docs/RELEASING.md).
