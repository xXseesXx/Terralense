# Structure applicability inventory

`structure-applicability.json` snapshots the installed TerraFirmaGreg Modern 0.9.25 / Minecraft 1.20.1 structure definitions and their raw biome gates in the existing seed-zero world's world-generation profiles. It was built from current Prism data, not the older `structures.json` snapshot (TFG Core 0.9.21).

The artifact includes all 66 structure sets and their 131 distinct structure definitions. `structure_sets` preserves each raw set definition. `structures` is indexed by actual structure ID and includes the source definition, sets that reference it, its raw biome selector, and per-dimension `biome_intersection`, `source_eligible`, and `scanner_eligible` gates. `sets` contains each placement and child weight. `biome_gate_tags` records raw values, recursively resolved IDs, installed resource sources, and KubeJS additions/removals. Input file names and SHA-256 hashes are included.

Nether, Moon, and Mars biome profiles come from the existing `level.dat` and match current KubeJS dimension profiles. The Overworld profile uses TFG Core 0.9.25's 109 TFG Earth biomes registered for the custom `tfc:overworld` source. A separate 139-ID registered-resource pool (109 TFG Earth plus 30 legacy TFC biomes) is retained for auditing; those legacy TFC IDs are not treated as generated profile IDs. Comparisons use raw registry IDs. No vanilla names are aliased to TFC names.

A biome intersection proves only that a structure's biome gate can match the dimension profile; it does not prove placement or terrain viability. `scanner_eligible` additionally requires a placement type supported by the scanner. Scanner callers must still enforce their per-type distance limits (currently requested as 100 km), frequency, height/terrain checks, and supported structure-ID filtering. Empty biome selectors and concentric-ring strongholds remain in the inventory but are gated out.

Regenerate without launching Minecraft or changing the world:

```sh
python3 -m pip install nbtlib
python3 tools/extract_structure_applicability.py \
  --minecraft-dir /path/to/instance/minecraft \
  --minecraft-client-jar /path/to/minecraft-1.20.1-client.jar \
  --reference-level /path/to/world/level.dat \
  --runtime-snapshot assets/data/tfg/installed-structure-runtime.json \
  --output assets/data/tfg/structure-applicability.json
```
