# Loot preview and Lootr behavior

TerraLens reads chest and Lootr SavedData while it builds a structure inspection in the
background engine. It opens saves read-only. The UI receives a completed inspection and never
reads chunk or Lootr files itself.

For a saved Lootr container, TerraLens derives its SavedData file from the container UUID using
Lootr's path layout:

```
<world>/data/lootr/<first UUID character>/<first two UUID characters>/<UUID>.dat
```

Lootr stores the block position and dimension in that record and stores each opened player's
inventory in a separate entry keyed by that player's UUID. TerraLens requires the record's
position and dimension to match the inspected container, then returns every stored player
inventory. A player record with an empty item list is known empty; no record means that player's
contents are not known. Missing records are never replaced with an invented roll.

## Exact prediction limits

The installed profile uses Lootr 0.7.35.94. Its `config/lootr-common.toml` currently has
`randomise_seed = true` and conversion enabled. The installed Lootr code returns a fresh
`ThreadLocalRandom` seed when it fills a Lootr inventory under this setting. It creates the
inventory on a player's first open and persists it by player UUID. Thus an unopened Lootr chest
has no exact shared item list derivable from world seed and location. Different players can get
different results. TerraLens displays its table's possible entries and stack ranges as a
preview, while marking the contents unknown until an actual player inventory is saved.

A non-Lootr chest with a saved nonzero `LootTableSeed` is also not presented as an exact roll:
faithful rolling requires Minecraft's complete loot context, predicates, tags, functions and RNG
implementation. TerraLens currently reports possible direct item entries and unresolved/context
caveats. A saved inventory, including a known-empty inventory, takes precedence as ground truth.

KubeJS-generated decorative vase loot is read from the installed `server_scripts/tfg/loot.js`
modifier. The app lists literal items in its three variable-roll pools; chance predicates are not
executed, so those are possibilities rather than guaranteed vase contents.

## Extract the bundled table catalogue

Run the maintenance extractor against the installed pack. It follows table references from the
structure loot audit and applies KubeJS-over-jar resource priority. Same-priority conflicts and
missing resources remain unresolved instead of choosing arbitrary data. Supply the official
Minecraft client jar to include vanilla structure tables when it is available locally:

```sh
python tools/extract_loot_tables.py \
  --minecraft-dir /path/to/instance/minecraft \
  --structure-loot-runtime assets/data/tfg/structure-loot-runtime.json \
  --minecraft-client-jar /path/to/minecraft-1.20.1-client.jar \
  --output assets/data/tfg/loot-tables.json
```

The extractor reads resources only; it does not launch Minecraft, execute KubeJS, open a world,
or modify saves. The runtime catalogue contains only reachable table resources and the supported
generated-vase summary.
