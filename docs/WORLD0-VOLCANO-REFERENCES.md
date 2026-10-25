# World 0 volcano references

Seed **0**, Overworld. All coordinates below are **X, Z** at centered features whose chunks
were already fully generated in world `0`. No chunks were generated for this check.

For orange lava in every layer, start with **13664, 14159**. Open world `0`, press Ctrl+G,
enter the coordinates and zoom to one block per pixel or closer. Detail mode can stay off.
The volcano navigation sprite fades at close scale so it does not cover the fluid raster.

For water-filled craters, try **1856,1147**, **14840,14327** or **-3191,5149**. Exposed
magma uses a darker orange than lava: examples are **13652,14158**, **-14062,40758**
and **-49485,92560**. All appear on the main map with detail mode disabled.

| Category | Saved centers found | Examples (X, Z) |
| --- | ---: | --- |
| Cinder cones | 53 | `13664, 14159`; `-14048, 40773`; `-49483, 92583` — all have exposed lava |
| Active shield vents | 1 | `-2754, 4513` — this center is water-filled |
| Dormant shield craters | 2 | `-50957, 89948`; `-50636, 89606` |
| Extinct shield craters | 4 | `19624, -3052`; `-50061, 92306`; `-99625, -21719` |
| Ancient shield craters | 0 | No saved example found |
| Sunken shield craters | 5 | `-96019, -21397`; `-96625, -21280`; `-96655, -21732` |
| Eroded shield-shore craters | 0 | No saved example found |
| Tuyas | 12 | `5518, -6365`; `8487, 26906`; `9135, 26408` — lava at each center |
| Glacial tuyas | 1 | `49954, 28225` — lava at the center |

The read-only scan inspected all 1,218 nonempty Overworld region files available at capture.
It examined 16,036 cellular feature candidates, found 3,724 fully generated center chunks,
and retained 78 centers matching the saved biome family and sampler rarity controls. Category
counts describe this saved area; absent categories remain supported in the viewer. Regional
volcano age and local active/dead cinder choice are separate controls, so the active-shield
biome does not guarantee that its particular centered feature contains lava.

The [complete reference report](data/volcano-reference-world0.json) includes all coordinates,
saved center biomes and top blocks. [Surface-lava validation](VOLCANOES.md#surface-lava)
separates exact saved-surface rendering from the more approximate seed-only cinder forecast.
