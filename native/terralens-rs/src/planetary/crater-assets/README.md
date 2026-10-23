# Moon crater generation provenance

The sampler ports TFG's four step-1 Moon crater placed features. Their nested geodes use air for
every layer, so the visible terrain effect is the vanilla `GeodeFeature` carving mask. Feature
order comes from the installed `tfg:moon_craters` tag: extra-large, large, medium, small. The source
chunk candidates are clipped to Minecraft 1.20.1 `WorldGenRegion`'s one-chunk write radius, and
each feature's heightmap origin includes earlier geodes from that same chunk. The large and extra
large entries are `tfc:if_then` wrappers around the recorded bowl geodes; their follow-up
meteorites do not change the carved surface.

`provenance.json` records SHA-256 hashes for the installed configured/placed feature JSON and tag
script, plus point-distribution vectors emitted by a standalone Java oracle using the installed
1.20.1 client classes. `crater_candidates_match_minecraft_1201_worldgenrandom_oracle` checks those
vectors against Rust's `FeatureRandom` stream and geode point construction. The oracle only loads
Minecraft classes; it does not launch the game or alter a world save.
