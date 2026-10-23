# Installed planetary generator data

These compact JSON assets capture the installed TerraFirmaGreg Modern Moon and Mars generator
profiles for the Rust planetary preview. `moon_dimension.json` and `mars_dimension.json` are byte
copies of the active KubeJS dimension resources. `resources.json` contains normalized JSON for the
datapack dependencies of the active `tfg:moon_noise` and `tfg:mars_noise` noise settings,
plus the corresponding Ad Astra dimension-type resources. It contains data definitions only; no
mod jars, saves, or generated chunks are included.

The active wrapper resources are `ad_astra:moon` and `ad_astra:mars` under
`kubejs/data/ad_astra/dimension/`. Moon's wrapper type is `tfg:moon`; Mars uses `ad_astra:mars`.
Both install `kubejs_tfc:wrapped` around a vanilla noise generator. Moon's inner noise setting is
`tfg:moon_noise`; Mars uses `tfg:mars_noise`. Their active density settings span Moon Y=0..383 and
Mars Y=-32..351 (minimum Y and height are in the noise settings).

`resources.json` is grouped by registry:

- `noise_settings`: two router documents.
- `density_functions`: 19 named functions reachable from those routers, including the TFG Mars
  terrain, Ad Astra's shared Moon functions, vanilla cave/density functions, and spline data.
- `noises`: six referenced named noise parameter documents.
- `dimension_types`: the Ad Astra Moon and Mars type documents, included as source metadata.
- `climate_profiles`: previously extracted Moon/Mars climate routers, vanilla noise parameters,
  and multi-noise biome ranges used for biome sampling. These include source metadata.

The climate-profile noise parameter tuples are needed for biome sampling. The named noise resources
in the `noises` map instead belong to terrain density generation. Built-in vanilla density IDs
such as `minecraft:overworld/continents`, `minecraft:overworld/ridges`, and `minecraft:shift_x`
have no standalone datapack JSON; their names are explicitly recorded in `provenance.json`.

The serialized density-function type inventory and per-resource origins are recorded in
`provenance.json`. Types appearing in the dependency closure include vanilla `old_blended_noise`
and `spline`, TFG `needs_post_processing`, and the KubeJS TFC `rock` wrapper type. The generator
parser should reject unsupported types explicitly; this inventory is not a claim that every
decoration or block-placement feature is simulated.

The Ad Astra dimension-type JSON files both say `min_y=-64,height=384`. The installed Moon
dimension wrapper instead names the custom type `tfg:moon`, whose physical range is Y=0..383.
Mars uses the Ad Astra type, whose physical range is Y=-64..319, while its noise grid begins at Y=-32.
Terrain uses the noise grid's alignment and the active dimension type's physical limits. The unused
TFG Mars type document does not define this world's build limits.

Vanilla built-in functions and noise parameters needed by the terrain sampler are supplied in
the adjacent `minecraft_1_20_1_*` files, extracted from the local Minecraft 1.20.1 resources and
bootstrap definitions. No Minecraft process is used at runtime.

## Provenance

The source profile is Minecraft 1.20.1, Forge 47.4.13, TFG Core Modern 0.9.25, TFC 3.2.25, and
Ad Astra 1.15.21. KubeJS resources take precedence for the installed wrapper and TFG noise settings.
Other registry documents are copied from the installed mod or Minecraft data resources. SHA-256
digests, relative resource names, versions, and registry-source paths are listed in
`provenance.json`; no machine-specific absolute paths are stored.
