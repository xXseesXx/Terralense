// Development-only reference against the installed Minecraft and TFC classes.
// Usage: java -cp <Minecraft-srg.jar>:<TerraFirmaGreg-Core.jar>:<dependencies> ForestNoiseReference <world-seed>
import net.dries007.tfc.world.noise.OpenSimplex2D;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;

public final class ForestNoiseReference {
    private static void consumeUniformLayer(RandomSource random, int zoomLevels) {
        random.m_188505_();
        for (int i = 0; i < zoomLevels; i++) {
            random.m_188505_(); // normal zoom seed
            random.m_188505_(); // smooth seed
        }
    }

    public static void main(String[] args) {
        long seed = Long.parseLong(args[0]);
        RandomSource worldRandom = new XoroshiroRandomSource(seed);
        worldRandom.m_188505_(); // RegionGenerator seed
        worldRandom.m_188505_(); // cellular noise seed
        worldRandom.m_188505_(); // continental noise seed
        worldRandom.m_188502_(); // temperature noise seed
        worldRandom.m_188502_(); // rainfall noise seed
        consumeUniformLayer(worldRandom, 2);
        consumeUniformLayer(worldRandom, 3);
        long chunkDataSeed = worldRandom.m_188505_();

        RandomSource chunkRandom = new XoroshiroRandomSource(chunkDataSeed);
        chunkRandom.m_188584_(chunkDataSeed ^ chunkRandom.m_188505_());
        chunkRandom.m_188505_(); // rock layer seed
        chunkRandom.m_188502_(); // layer height seed
        chunkRandom.m_188502_(); // layer skew X seed
        chunkRandom.m_188502_(); // layer skew Z seed
        chunkRandom.m_188505_(); // forest type layer seed
        int weirdnessSeed = chunkRandom.m_188502_();
        int densitySeed = chunkRandom.m_188502_();

        OpenSimplex2D weirdness = new OpenSimplex2D(weirdnessSeed).octaves(4).spread(0.0025f);
        OpenSimplex2D density = new OpenSimplex2D(densitySeed).octaves(4).spread(0.0025f)
            .scaled(-0.2f, 1.2f);

        System.out.printf("seed=%d chunk_data=%d weirdness_seed=%d density_seed=%d%n",
            seed, chunkDataSeed, weirdnessSeed, densitySeed);
        int[][] chunks = {{-141, -145}, {-130, -222}, {0, 0}, {31, -64}, {-1, 1}};
        for (int[] chunk : chunks) {
            double x = (double) (chunk[0] * 16 + 8);
            double z = (double) (chunk[1] * 16 + 8);
            double w = Math.max(0.0, Math.min(1.0, 1.1f * Math.abs(weirdness.noise(x, z))));
            double d = Math.max(0.0, Math.min(1.0, density.noise(x, z)));
            System.out.printf("chunk=(%d,%d) density=%.9g weirdness=%.9g%n",
                chunk[0], chunk[1], (float) d, (float) w);
        }
    }
}
