# Installed structure availability

The current TFG Core 0.9.25 profile contains **66 placement sets and 131 distinct structure
registry IDs**. TerraLens uses the installed definitions and literal biome tags, including
KubeJS changes. Catalog discovery and prediction support are separate: registered definitions
remain in the appropriate dimension's controls even when this sampler has no matching biome
or audit witness. Similar biome names in different namespaces are not interchangeable in
placement checks. A negative sample is never a catalog blacklist.

The seed-zero audit searches placement cells inside a **100,000-block radius around (0,0)**,
without viewer LOD or marker-count limits. Each concrete weighted member stops after its first
qualifying candidate. Definitions without a literal intersection with the pinned sampler's
biomes receive a diagnostic no-match record. That record describes the static snapshot;
it does not establish universal generation impossibility or remove a viewer control.

| Dimension | Types with a candidate | No matching sampler biomes |
| --- | ---: | ---: |
| Overworld | 30 | 101 |
| Beneath | 3 | 128 |
| Moon | 4 | 127 |
| Mars | 0 | 131 |

These are per-dimension counts of the same 131-type inventory. Across all four dimensions,
37 types have preview witnesses and 94 have no literal match in those sampler profiles.
These numbers measure native preview support, not the number of registered controls or every
structure Minecraft can generate. Every member with a matching source biome was found within
the radius. No type is disabled because this audit did not find it. The complete machine-readable [audit](data/structure-audit-seed0-100km.json) records each
type, rejection reason, candidate location, and number of cells checked.

## Snapshot differences

- Vanilla strongholds, ancient cities, villages and most other vanilla structures reference
  vanilla biome IDs/tags. None intersect this profile's 109 `tfg:earth/*` biomes. Strongholds
  also use concentric-ring placement, which the seed scanner does not implement. These
  definitions remain in the Overworld controls; they are not assigned fabricated preview
  positions.
- Primal Ancient Beasts (`wan_ancient_beasts`) structures also have no matching biome IDs in
  the supported current profiles. A structure definition existing in a mod is insufficient.
- Nether fortress has no TFG biome extension. Bastions and Nether fossils explicitly receive
  `#tfg:nether_biomes` through KubeJS and remain available, together with the TFG Beneath tower.
- `tfg:beneath/outpost` has an explicitly empty `biomes` array in the installed pack. The
  separately named `____biomes` property is not an active structure biome selector.
- Stock Ad Astra Moon dungeons and Mars temples reference stock Ad Astra biomes. Those are
  absent from the custom TFG planetary sampler profiles. The stock controls remain available
  on their corresponding planets. Four TFG Moon structures have preview witnesses; no current
  Mars structure has a matching native preview biome.

The right rail and display options list registered, dimension-appropriate types independently
of audit success. The placement scanner uses its narrower supported biome profile. Only an
explicit empty structure biome selector is omitted as a pack disable. Markers identify the
concrete selected structure, so expanded group toggles control individual
variants. Failed weighted children remain in the placement draw to preserve random consumption.
Saved structure starts remain independent evidence: an actual recorded start can still be shown
from an older or customized save even if the current pack would not place another one.

## Evidence and limits

A candidate passes the implemented placement, weighted-member, biome and climate/forest gates.
**It is not proof of a successfully generated Minecraft structure start.** Terrain checks,
jigsaw assembly and mod-specific generation code can still reject it. The audit does not
launch Minecraft or generate chunks, and a single seed is not a universal absence proof.

The same report separately includes read-only Anvil evidence from world `0`: 278 Overworld
starts across 25 distinct types and 4,295 Moon starts across all four Moon types, with bounds
and zero decoding errors. The scan examined 74,474 Overworld, 36,761 Moon and 13,399 Mars chunks.
Mars had no recorded starts. This save contains no generated Beneath region files, so there
is no independent saved-start confirmation for that dimension in this report.

The current Overworld source pool contains 109 TFG Earth IDs. The installed resources also
register 30 legacy TFC IDs, retained separately in the inventory; worldgen v0 is unsupported
and those IDs are not added to the current pool. Older OutCrop 0.9.21 fixtures retain their
12 exact placement/selected-child witnesses. Their population counts differ because the
current raw definitions do not contain the older normalized height/avoid conditions.

## Cross-save follow-up

A later read-only check of all four local Overworld saves found **1,048 starts in 453,668
generated chunks**, with zero decode errors. Every recorded registry ID was already present
in the exact-profile catalog; those starts included TFG Norse/Arabic villages, temperate
houses and both ruined towerhouse types. This does not validate the old catalog blacklist:
registered entries without saved examples must remain available as well. The current policy
restores them and keeps saved evidence independent from preview support.

## Reproduce

The [source inventory](../native/terralens-rs/assets/data/tfg/STRUCTURE-APPLICABILITY.md)
documents extraction and input hashes. Its extractor is a maintenance tool; the application
embeds the tables and needs neither Python nor the author's Prism installation.

From the repository root:

```sh
cargo run --release --locked --features diagnostics \
  --manifest-path native/terralens-rs/Cargo.toml --example structure_audit \
  -- docs/data/structure-audit-seed0-100km.json
```

Candidate checks use the pinned source profile at seed zero. Optional saved-start evidence
uses already-generated seed-zero worlds in the locally discovered Prism instance; absent
saves do not prevent candidate auditing. This command never launches Minecraft or writes to
saves. Run `./build.sh` afterwards to restore the ordinary release without diagnostic tools.
