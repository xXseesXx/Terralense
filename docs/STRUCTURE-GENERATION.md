# Structures in the installed TFG worldgen profiles

TerraLens's structure controls use an explicitly reviewed compatibility list for TFG Core
0.9.25 / Minecraft 1.20.1. The list is based on effective structure sets, biome selectors,
KubeJS tag changes and the pack's actual generator profiles. **A miss in a 100 km scan is
not a reason to remove a structure.** Merely installing a mod is not evidence that its
structures can generate in TFC's custom Overworld either.

The exact registry IDs are in
[`structure_support.rs`](../native/terralens-rs/src/structure_support.rs). The policy keeps
every child of the supported families, including less common village, church and house
variants. Structure-set aliases are not counted as additional buildings.

| Overworld family | Decision | Installed-pack reason |
| --- | --- | --- |
| TFG Arabic, Malay, Norse and yurt villages | Keep all five registry variants | Explicit TFG climate placement sets and TFC/TFG biome tags, including living and ruined Norse variants. |
| TFG Plains Temperate house | Keep | Explicit TFG house set, biome/climate gates and its house template pool. |
| TFC Ruined World | Keep all 16 variants | Pack-supported monuments, churches, towers, towerhouses and castles with TFG Earth biome selectors. |
| TFC Ruins | Keep all three variants | Pack-supported small, beach and rich ruins. |
| TFG camps, forest roaming, mineshaft, aqueduct and ocean moai | Keep | Explicit TFG structure definitions and compatible Overworld biome selectors. |
| Vanilla structures, including vanilla villages and strongholds | Cull | Their vanilla biome gates are not extended to the 109 TFG Earth biomes used by this TFC worldgen profile. TFG villages and mineshafts are separate registry types. |
| Primal / Ancient Beasts (`wan_ancient_beasts`) | Cull | The pack explicitly clears its structure biome tags in `kubejs/server_scripts/wan_ancient_beasts/tags.js`. |
| Species structures, AE2 meteorite, AlekiShips boats and Arthropocolypse desert nest | Cull | Installed selectors require other biome IDs; the pack supplies no TFG Earth extension enabling them. |

That leaves **30 Overworld registry types**. All five TFG village variants, the temperate
house, both towerhouses and every supported church/monument variant are covered by explicit
regressions. Ruined variants remain in the Ruins group and follow its user visibility setting;
being hidden is distinct from being incompatible with worldgen.

Other supported dimensions are reviewed separately:

- **Beneath:** TFG tower, Bastion remnant and Nether fossil. KubeJS extends the Bastion/fossil
  biome tags to TFG Nether biomes. Vanilla fortress has no such extension; the TFG outpost
  has an explicitly empty structure biome selector.
- **Moon:** TFG cheese ores, meteors, rabbit houses and Moonbase. Stock Ad Astra structures
  require stock Ad Astra biomes rather than this pack's TFG Moon biomes.
- **Mars:** no currently installed structure has a compatible TFG Mars selector.

## Sampling and independent saved evidence

The diagnostic [seed-zero report](data/structure-audit-seed0-100km.json) contains all 66 sets
and 131 distinct structure IDs across four dimensions. It searches an origin-centered
100,000-block circle, stopping independently at the first qualifying placement for each
weighted member. It found witnesses for all 37 reviewed compatible types: 30 Overworld,
three Beneath and four Moon. These results validate the sampler; they do not define the
compatibility list.

A candidate passes implemented placement, weighted-member, biome and climate/forest gates.
It is not proof of a successful Minecraft structure start: terrain checks and jigsaw assembly
can still reject it. Unknown generation details must not be replaced with guessed positions.

The pinned report separately records 278 Overworld starts across 25 types and 4,295 Moon
starts across all four Moon types, with bounds and zero decode errors. A later read-only
check of all four local Overworld saves found **1,048 starts in 453,668 generated chunks**,
also without decode errors; every recorded type is retained by the reviewed list. Saved
starts are independent evidence and are always readable, including starts in older/custom
worlds from types that the current pack would no longer generate.

The application's catalog does not read scan results, and auditing does not run at startup.
Normal viewing needs neither Minecraft nor Python. The current Overworld sampler uses 109
TFG Earth biome IDs; legacy TFC worldgen v0 is a different unsupported profile.

## Reproduce the diagnostic scan

The [source inventory](../native/terralens-rs/assets/data/tfg/STRUCTURE-APPLICABILITY.md)
documents extraction and input hashes. From the repository root:

```sh
cargo run --release --locked --features diagnostics \
  --manifest-path native/terralens-rs/Cargo.toml --example structure_audit \
  -- docs/data/structure-audit-seed0-100km.json
```

Optional saved evidence uses already-generated seed-zero saves in the locally discovered
Prism instance. This command never launches Minecraft or writes to saves. Run `./build.sh`
afterwards to restore the ordinary release without diagnostic tools.
