# Loot-aware structure groups

TerraLens reserves **Ruins** for ruined-looking structures whose complete reachable template
and processor graph is verified to have no loot and no containers. Names and mod namespaces
are not sufficient. A ruined building with chests, loot vases, filled vessels or other loot
stays visible in **Ruined buildings**, with a small chest detail on its themed map icon.
Unresolved ruined-looking structures also stay in Ruined buildings; uncertainty must not
silently hide a potentially useful site. Dead Norse villages belong with **Villages**.

The old name-based hiding is removed once when upgrading preferences. Verified lootless
ruins retain their hidden default, as do separately configured frequent features. Later
individual and group visibility choices persist.

## Ground truth: world 0, chunk 1231, -204

The saved start is `tfc_ruins:ruin_small`, with template `tfc_ruins:ruin_small/ruin_8` and
processor list `tfc_ruins:ruin_small`. It is a mud-brick building with two populated, sealed
large vessels:

| Container position | Saved contents |
| --- | --- |
| 19698, 71, -3259 | 6 straw and 6 small sphalerite ore |
| 19697, 74, -3261 | 18 clay balls |

The vessel blocks are `tfc:ceramic/large_vessel`; their block-entity type is `tfc:large_vessel`.
The stored inventories contain actual items, without a deferred `LootTable` field. Looking
only for chest blocks or loot-table NBT would miss this evidence.

Its inclusive saved bounds are X19696–19703, Y69–75, Z−3264–−3258. The map correctly uses the
footprint center **X19700, Z−3260.5**. Both the saved reader and seed predictor identify the
structure. Its former absence was caused by the default-hidden Ruins classification, rather
than a missing structure start. The portable independent
[observation fixture](../native/terralens-rs/tests/fixtures/loot-sites/world0-1231--204.json)
regresses this exact case against the template-derived classification.

## Inventory audit

The [full audit](../native/terralens-rs/assets/data/tfg/structure-loot.json) covers all **131
registered structure definitions**, including structures excluded from the current pack's
generation profile. It records 77 with confirmed reachable loot, 23 with exhaustive no-loot
evidence, and 31 with unresolved or code-defined behavior.

For the structures compatible with the supported profiles:

| Dimension | Confirmed loot | Complete no-loot evidence | Unknown |
| --- | ---: | ---: | ---: |
| Overworld | 11 | 19 | 0 |
| Beneath | 2 | 0 | 1 |
| Moon | 1 | 3 | 0 |
| Mars | 0 | 0 | 0 |

These are generation-evidence classes, not a count of members of the Ruins group. Camps,
meteors, cheese ores and other objects keep their own semantic categories regardless of
container status. Specifically:

- All three `tfc_ruins` types—small, beach and rich—have populated vessel templates and
  belong in visible Ruined buildings.
- The 16 `tfc_ruined_world` church, monument, tower, towerhouse and castle variants contain
  no containers or block-entity NBT, have complete template closure and no processors.
  Aqueducts and ocean moai also have complete no-loot evidence and remain in Ruins.
- The temperate house contains generated decorative vases. KubeJS block loot modifiers
  confirm their loot even when the structure NBT does not specify a loot table.
- TFG mineshafts have chests, vessels, vases and three explicit loot tables. Unreviewed
  custom mine processors/features remain recorded as caveats; they do not erase positive
  loot evidence. Nether fossils have code-defined placement without a data-defined pool
  and remain unknown in this audit.

The extractor resolves KubeJS overrides, start/fallback pools, jigsaw connector targets,
list and delegated/limited elements, alternate template palettes, nested `Items` inventories,
placed container items, processor-supplied loot and the pack's generated-vase loot modifiers.
Pool reachability over-approximates random assembly: a type with possible loot does not
promise that every individual generated instance contains every reachable container.
Unknown procedural code, unsupported processors and missing resources are never treated as
proof of a lootless structure.

Classification embeds the compact [runtime summary](../native/terralens-rs/assets/data/tfg/structure-loot-runtime.json)
(about 52 KB). It loads once; the full evidence remains a maintenance/review artifact.
The [layout inspector](STRUCTURE-PREVIEWS.md) additionally bundles template geometry and reachable
loot tables, and reads saved containers on demand in the Rust engine. No Python or Minecraft
process runs during viewing.
Feature search and the Structures table can match container types such as `vessel` and `vase`,
as well as `loot`. Dead villages also match `dead village` and `abandoned village`.

## Reproduce

Use an isolated Python environment with `nbtlib` installed, then from the Rust crate directory:

```sh
python tools/extract_structure_loot.py \
  --minecraft-dir /path/to/instance/minecraft \
  --minecraft-client-jar /path/to/minecraft-1.20.1-client.jar \
  --structure-applicability assets/data/tfg/structure-applicability.json \
  --output assets/data/tfg/structure-loot.json
```

This writes both the full audit and its compact runtime sibling. Source SHA-256 hashes and
resource-relative paths are recorded. Extraction reads pack resources only; the supplied
saved-chunk observation was collected separately with read-only Anvil decoding.
