# Surface caves and aquifers

Local Overworld views (up to four blocks per pixel) resolve the physical exposed
terrain instead of assuming every raw-height column is solid. Ground materials,
fluid colors, relief and contours use the resulting solid floor. Water/lava remain
visible across map layers. Opened-world surface blocks retain priority. Regional
overview tiles use the existing inexpensive height pass.

The seed-only Rust sampler models these installed TFG 0.9.25 / TFC 3.2.25 stages:

- Blended biome underground-lake density and three-dimensional river profiles,
  including the undercut whorled-canyon walls.
- The installed five cave/noodle density functions on their 4×8×4 interpolation
  lattice, interpolating X, then Z, then Y.
- TFC cave, deep-cave, karst-cave and canyon trajectories from the surrounding
  17×17 source chunks, with positional Xoroshiro seeds and legacy tunnel RNG.
- TFC aquifer positions, local ceiling minima, cellular fluid selection,
  pressure barriers, water and lava levels.
- Separate surface-builder and post-carver caps. A newly exposed floor retains
  subsurface rock/soil/sandstone instead of receiving a fresh grass layer.

This supplies surface openings and flooded floors on the normal map; it adds no
underground interface. Snow and ice are outside the requested accuracy effort.

The generation cache is bounded and keyed by world seed and all region settings.
It has no save-file dependency. Profile extraction and original Java oracle tools
are under `native/terralens-rs/tools/reference/`. Their generated JSON and source
fingerprints are bundled, so Java and a source checkout are unnecessary at runtime.

Verification uses 48 independently decoded World 0 chunks, plus 512 direct calls
to the installed original Java simplex/cellular implementations over four seeds
and signed coordinates. The Java cellular outputs match exactly; river-wall noise
matches within 0.000002. Saved whorled-canyon openings also check exact floor Y
and sandstone variant. The broader scores, class precision/recall and failures
are recorded in [generator accuracy](GENERATOR-ACCURACY.md).

Remaining differences include structure density adjustments, every specialist
subsurface builder, material-specific carve replacement, adjacent decoration
order and later features that fill or excavate terrain. The sampler does not
construct complete Minecraft chunks. In particular, the 99.36% voxel total is
not an accuracy claim for rare openings. The later [block-identity audit](SURFACE-MISMATCH-SCENARIOS.md)
shows that most disagreements are cave spikes, calcite and hardened rock placed
into space that was carved earlier; they must not all be called cave-shape errors.

The final release engine completed 22 tile/layer measurements with byte-identical
RGBA/biome/water payloads before and after cache optimization. Sharing finished
relief across worker threads reduced the new pass's measured block-scale layer
switches from 0.44–0.71 s to 0.036–0.22 s in these controls. This comparison is
against the initial cave implementation, not a claim that all work is faster than
the previous height-only release. The extra local density work has a cold cost.
The [full measurements](data/surface-caves-engine-performance.json) and
[pre-cache control](data/surface-caves-engine-before-cache.json) exclude GPU
presentation and do not diagnose desktop not-responding notifications.
