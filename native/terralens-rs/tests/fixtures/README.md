# Native parity fixtures

`ganymede-observations.json` contains independent data from the installed Minecraft save:
1,556 stored quart biomes, 371 TFC chunk climate samples, 371 forest ordinals, 371 generated
surface heights and 370 observed raw-rock blocks. The seed is a decimal string to preserve
all signed 64-bit bits. Captures read saves without changing them. Region digests belong to
the original biome/climate capture; forests, heights and rocks were read from the same save
again during the Rust port. Height remains an approximation with the previous measured
95th-percentile error of five blocks and maximum error of 19 blocks.

`tfg-reference.json` is a conversion reference captured from the existing TypeScript
generator, **not** independent Minecraft evidence. It covers three seeds, 274 locations per
seed, signed chunk/quart/grid boundaries, positive and negative coordinates, full probes,
and three 512-by-512-block feature windows (715 deposits and 12 structure candidates).
Source SHA-256 values identify the relevant source/data inputs. This file is immutable test
input: Rust tests do not invoke JavaScript or regenerate expected results.

The TypeScript capture originates from [OutCrop](https://github.com/PauAixala/outcropmap),
pinned at `7d5b7940d96cbe134001416b09ea13862dba2598`. The original fixture metadata retains
source hashes and provenance. Regeneration is a maintainer task performed against that reference;
normal tests use only the committed data and require neither Node nor Minecraft.

The JVM/Minecraft RNG captures are vendored in `core/`; their `source` fields record the Java
capture. Climate gates compare exact float bits, and biome/forest/geology/height conversion gates
compare exact outputs. Gaussian streams alone permit a small transcendental-library tolerance.
