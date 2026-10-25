# TFC climate colormaps

`grass.png`, `foliage.png`, `foliage_fall.png`, `foliage_winter.png`, and `water.png` are copied unchanged from TerraFirmaCraft's
`src/main/resources/assets/tfc/textures/colormap/` in the installed TerraFirmaGreg
Core 0.9.25 / TFC 3.2.25 source. They are upstream game assets,
not TerraLens artwork, and are licensed under EUPL-1.2. The source project's copyright
and license notices are retained in its repository and in [NOTICE.md](../../../../NOTICE.md).

SHA-256 checksums of the unchanged PNG files:

| File | Dimensions | SHA-256 |
|---|---:|---|
| `grass.png` | 256 × 256 RGB | `289451d8c948b347b835d6383f405d606ab3f3eb133352bdd29671719b65949d` |
| `foliage.png` | 256 × 256 RGB | `d6bac38080bd747f1f024ee5ff63587a055fd44df36b13c5e5195e9c19c243ad` |
| `foliage_fall.png` | 256 × 256 RGB | `5a32da320ec3691911eeea5c74a3e710d13f0699f490f8f87f94c8fb63b55c05` |
| `foliage_winter.png` | 256 × 256 RGB | `f0c30aac734ce762d857ea9a79463ef00e9052c36e10d62499956efee03215b0` |
| `water.png` | 256 × 256 RGB | `dcd96bbedd3e21dadaafeb0ea5b37aa1b52d0673e5de7045bf3e2267a4da0af4` |

`foliage_{jacaranda,yellow,orange,red}.png` are copied unchanged from the
installed ArborFirmaCraft 1.0.22 JAR at `assets/tfc/textures/colormap/` and are
licensed under its installed MIT license ([copyright and permission notice](../../../../licenses/ARBORFIRMACRAFT-MIT.txt); author: Therighthon; the installed package credits
Dodo for assets and contributors for translations). `afc-species-indices.json` records the AFC
wood/tree species color index and conifer flags extracted from the installed
enum constructors (the Rust lookup uses those values directly).

The sampler follows `TFCColors.getClimateColor` in the pinned installation: integer,
truncated climate indices with no interpolation. Average temperature is elevation-adjusted
as in TFG's `OverworldClimateModel`. Seasonal sampling uses the installed fall and winter
palettes, TFC's seasonal-leaves tag and autumn timing, AFC blossom palettes, TFG palm
lifecycle colors, and TFG's monthly temperature curve for grass, ordinary foliage, and
water. Daily weather and hour-of-day temperature noise are omitted for stable averaged
colors. AFC's month-transition positional hash is retained because it defines where its
blossom phases change in the source implementation.
