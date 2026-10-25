# Structure marker artwork

The `cubiomes/` PNGs are copied, without changing their file contents, from Cubiomes Viewer:
https://github.com/Cubitect/cubiomes-viewer/tree/3acc863245b30c655498d60323c50da0865e0199/rc/icons

Upstream commit: `3acc863245b30c655498d60323c50da0865e0199`.
Copyright: Cubitect and Cubiomes Viewer contributors; upstream credits Amidst for icon inspiration.
License: **GPL-3.0**. The upstream license is retained at `licenses/GPL-3.0.txt` in this repository.
Upstream source, license and credits:

- https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/LICENSE
- https://github.com/Cubitect/cubiomes-viewer/blob/3acc863245b30c655498d60323c50da0865e0199/README.md#legal-information

`cubiomes/provenance.json` records every copied path and SHA-256 checksum. No Cubiomes source code
or dependency is used at runtime. TerraLens renders these sprites with nearest-neighbor scaling,
removes any fully black two-pixel rectangular frame, and treats near-zero alpha as transparent.
The upstream source files remain unmodified.

TFG-specific firepit, primitive village head, cracked limestone brick, church cross and timber
mineshaft arch are original TerraLens pixel-art code in `src/structure_icons/tfg.rs`, added on
23 October 2026 under EUPL-1.2. Other original symbols remain in `src/structure_icons.rs`.
They have transparent backgrounds, and their visible object silhouette defines the marker shape.

Ruined churches use broken chapel masonry with an open arch, ruined towers/towerhouses/castles
use a shattered stone keep, and ancient monuments use weathered standing stones. These original
32px sprites live in `tfg.rs`; generic ruins and the shared Ruins group use cracked limestone.
The group spans mod namespaces while its expanded children and map pins retain their topic icons.

Moon bases use an original dark gray pressure-dome sprite, and Moon rabbit houses use an original
rabbit-in-a-nest-box sprite. Both are crisp 32×32 pixel art in `src/structure_icons/tfg.rs` and are
shared by map markers, the icon rail and exports.

The Moon cheese structure uses the Ad Astra cheese **item** image in personal builds. The optional
source is `assets/ad_astra/textures/item/cheese.png` from the user's installed Ad Astra JAR. Place
that PNG at `native/terralens-rs/local-assets/ad-astra-cheese.png`, or set
`TERRALENS_AD_ASTRA_CHEESE` to its PNG path before building. `build.rs` embeds it in the binary;
the viewer performs no runtime scan or texture-file read. The game PNG is kept out of Git:
Ad Astra's Terrarium license reserves its non-code artwork. Public source-only builds without the
local image use an original cheese-wedge sprite from `tfg.rs`.

The icon-inclusive application is distributed under GPL-3.0 using EUPL-1.2's compatible-license
clause. Original source files retain their EUPL-1.2 notices. Both license texts and the full source
are available with the repository. See `NOTICE.md` for other bundled artwork and dependencies.

Loot-bearing ruined buildings retain their masonry/church/tower/monument topic and add an
original small chest silhouette. Loot evidence comes from the installed template/processor
audit, not the registry name. The canonical Ruined buildings group uses that chest-accent art;
lootless Ruins retain the plain broken-stone symbols. No image backdrop is added.

Volcano markers use original transparent pixel art: an active cinder cone with its lava vent,
a quiet shield cone, an open tuff ring, and an ice-covered cone. The symbols are generated in
`src/structure_icons/tfg.rs`; marker IDs select the state-specific silhouette without adding a
badge or background.
