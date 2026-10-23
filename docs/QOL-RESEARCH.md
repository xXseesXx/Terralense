# Amidst and Cubiomes Viewer: QoL research for TerraLens

Researched 22–23 October 2026 against official project documentation and source. Cubiomes Viewer
source inspected at `3acc863245b30c655498d60323c50da0865e0199` (trunk); Amidst references use
its official master branch. This records the original research priorities. The subsequent native implementation now includes
coordinate navigation/history, saved spots/collections/sessions, ruler/grid/selection, persistent
land-only highlighting, display presets and per-type filters, scoped searches with coverage,
regional analysis, multi-condition fixed-seed site search, PNG/CSV/JSON exports and Xaero sharing.
Multi-seed condition searches and Lua scripting remain future work. Priority and
effort below are engineering judgments for TerraLens; they are not upstream claims or time estimates.

TerraLens already supplies unified Ctrl+F search, nearest-distance ordering, interactive feature
pins, clustering, connected-region hover, climate/terrain layers and configurable tooltips. The
most useful additions build on those capabilities. GUI export, sessions, view history and bookmark
collections are implemented. The local Prism instance includes Xaero World Map and Minimap.

The second polish pass adapts Amidst's selectable biome key and Cubiomes' seed-list and image-export
workflows. It keeps one sidebar workspace visible, makes the toolbar responsive, adds a named seed
shortlist, and uses only cached tile data for the interactive map legend. PNG composition runs in
the existing engine, including marker artwork, recorded structure footprints, coordinates, scale
and a layer key. Site-finder results retain the completed request and the positions of matching
sampled rock and predicted ore; selecting a site connects those evidence points on the map.

These are adaptations for this pinned TFG generator. They do not claim Cubiomes' vanilla-version
coverage, general multi-seed condition engine, Lua filters or exact unseen chunk generation.

| Priority | Verified upstream behavior | TerraLens adaptation | Effort |
| --- | --- | --- | --- |
| First | Both have coordinate-jump actions; Cubiomes uses Ctrl+G. | Dedicated coordinate dialog accepting `X Z` and `X, Z`; retain zoom, optional zoom input. Add back/forward navigation for jumps, search results and world changes. Back/forward is our proposed extension. | Small–medium |
| First | Cubiomes has a Shift-based distance ruler; Amidst offers grid and scale toggles. | Ruler in blocks/km, scale bar, adaptive coordinate grid, optional chunk/region boundaries. These consume cached geometry and require no generation scans. | Small |
| First | Cubiomes context menu copies coordinates, chunk/region coordinates, seed and teleport commands. | Right-click anywhere for copy formats, pin and search-from-here. Add clipboard/file waypoint export for the installed Xaero mods, keeping exports separate from automatic save edits. Xaero integration is our extension. | Small–medium |
| First | Cubiomes saves/loads search sessions, restores previous sessions and keeps recent seeds; Amidst can log seed history. | Named sessions with view, filters, search settings, pinned places, notes and favorite seeds. Persist multiple bookmarks per world/settings identity. Named map bookmarks are our extension. | Medium |
| First | Amidst supports selectable biome highlighting and reloadable biome color profiles. | Persistent highlight masks for chosen biomes, rocks and climate ranges, alongside existing hover. Supply readable palettes and legends; keep ocean colors consistent and exclude ocean from land filters. | Medium |
| First | Both expose layer shortcuts; Cubiomes map tools allow individual feature toggles and visibility scales. | Layer shortcuts when the map has focus, per-structure and per-ore toggles, zoom thresholds and saved presets such as exploration, prospecting and farming. Preserve shortcut behavior inside text fields. | Small–medium |
| Next | Cubiomes conditions and analysis can take bounds from the visible map; selected conditions can show their area. | Search this view/selection, draw requested radius and actual scanned footprint, and show partial coverage. Saved starts cover the indexed save, while existing ore and terrain searches have smaller prediction limits. | Medium |
| Next | Cubiomes Locations evaluates multiple relative conditions inside one seed. | TFG base-site finder: land, user-specified temperature/rainfall, limestone within 1 km and a predicted copper vein within 2 km. Show which condition matched where. Begin with existing sampled fields and supported predictions. | Large |
| Next | Cubiomes analyses report biome distribution and structure locations/counts, with export. | Analyze a selected land area: biome/rock shares, climate ranges, tree habitat estimates and nearby ore/structure candidates. Export CSV/JSON with sampling resolution and prediction/confirmation labels. Exclude oceans from land summaries. | Medium–large |
| Next | Amidst distinguishes screenshot and biome-image export; Cubiomes offers screenshots and bounded/tiled image export. | GUI export current view or a selected region, with chosen layer, resolution, legend, seed/settings and markers. Use a cancellable background job and predictable memory limits. Existing Rust CLI rendering is a starting point. | Medium |
| Later | Cubiomes has seed lists, search presets, logical helpers and custom Lua filters. | Reusable search recipes and comparison shortlists; later, a budgeted multi-seed finder. Implement useful fixed-seed conditions before exposing a general scripting system. | Large |

Implementation order: coordinate navigation and ruler/grid; right-click actions and bookmark/session
storage; highlight/layer presets; visible-area search and analysis; multi-condition location finding.
The last provides the largest TFG-specific capability, while the first group improves every session.

All sampling, indexing, export and condition evaluation should remain in the existing engine
process. UI-only changes should use cached map data. New scan jobs need cancellation and work
limits; long jobs should expose coverage/progress and a stop action. Caches must include the full
seed and generation settings. Sampled climate, estimated tree counts and predicted deposits should
retain their current accuracy labels. TFC's climate and terrain backend remains the source of data
for all adapted workflows.

Primary evidence:

- [Amidst capability overview](https://github.com/toolbox4minecraft/amidst#what-is-amidst).
- [Amidst menu actions](https://github.com/toolbox4minecraft/amidst/blob/master/src/main/java/amidst/gui/main/menu/AmidstMenuBuilder.java) and [shortcuts](https://github.com/toolbox4minecraft/amidst/blob/master/src/main/java/amidst/gui/main/menu/MenuShortcuts.java): coordinate/spawn navigation, layers, grid, scale and profiles.
- [Amidst biome selection](https://github.com/toolbox4minecraft/amidst/blob/master/src/main/java/amidst/gui/main/viewer/BiomeSelection.java) and [biome widget](https://github.com/toolbox4minecraft/amidst/blob/master/src/main/java/amidst/gui/main/viewer/widget/BiomeWidget.java): persistent selected-biome highlighting.
- [Amidst actions](https://github.com/toolbox4minecraft/amidst/blob/master/src/main/java/amidst/gui/main/Actions.java): screenshot and biome-image export.
- [Amidst seed history](https://github.com/toolbox4minecraft/amidst/blob/master/src/main/java/amidst/mojangapi/world/SeedHistoryLogger.java): seed history logging.
- [Cubiomes overview](https://github.com/Cubitect/cubiomes-viewer#basic-feature-overview): relative conditions, fixed-seed Locations and biome/structure analysis.
- [Cubiomes map interactions](https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/src/mapview.cpp): coordinate jump, copy formats, distance ruler and screenshots.
- [Cubiomes menus](https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/src/mainwindow.ui), [session/history code](https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/src/mainwindow.cpp), and [restore/autosave settings](https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/src/configdialog.ui).
- [Cubiomes map tool configuration](https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/src/maptoolsdialog.ui): feature controls and maximum visible scale.
- [Cubiomes 4.0 changes](https://github.com/Cubitect/cubiomes-viewer/releases/tag/4.0.0): fixed-seed Locations, biome proportions, selected-condition outlines and visible-area input.
- [Cubiomes biome analysis](https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/src/tabbiomes.cpp) and [structure analysis/export](https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/src/tabstructures.cpp).
- [Cubiomes image export dialog](https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/src/exportdialog.ui).
