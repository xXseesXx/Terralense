import net.dries007.tfc.world.noise.Noise2D;
import su.terrafirmagreg.core.world.new_ow_wg.noise.TFGBiomeNoise;

/** Direct oracle over the installed TFG Core Modern 0.9.25 class. */
public final class TerralensShieldHeightOracle {
    private static final long[] SEEDS = {0L, 1L, -1L, 123456789L};
    private static final int[][] POINTS = {
        {0, 0}, {-1, 1}, {1, -1}, {-1808, 5232}, {5504, -6368},
        {12345, -23456}, {-98765, -4321}, {32000, 32000}
    };
    private static final double[] THRESHOLDS = {0.45, 0.70, 0.73, 0.85};
    private static void n(StringBuilder b, String k, double v, boolean comma) {
        b.append("\"").append(k).append("\":").append(Double.toString(v)); if (comma) b.append(',');
    }
    private static void i(StringBuilder b, String k, long v, boolean comma) {
        b.append("\"").append(k).append("\":").append(Long.toString(v)); if (comma) b.append(',');
    }
    public static void main(String[] args) {
        StringBuilder b = new StringBuilder(32000);
        b.append("{\n  \"format\":\"terralens-shield-height-java-v1\",\n")
         .append("  \"provenance\":{\"class\":\"su.terrafirmagreg.core.world.new_ow_wg.noise.TFGBiomeNoise\",\"method\":\"dormantShieldVolcano(long, Noise2D)\",\"mod_version\":\"TerraFirmaGreg-Core-Modern-0.9.25\",\"invocation\":\"installed class direct\",\"Mth_stub\":\"bytecode-equivalent inverseLerp then lerp\"},\n")
         .append("  \"samples\":[\n");
        boolean first = true;
        for (long seed : SEEDS) {
            Noise2D hot = TFGBiomeNoise.dormantHotSpots(seed);
            Noise2D height = TFGBiomeNoise.dormantShieldVolcano(seed, hot);
            for (int[] p : POINTS) {
                if (!first) b.append(",\n"); first = false;
                b.append("    {"); i(b,"seed",seed,true); i(b,"x",p[0],true); i(b,"z",p[1],true);
                n(b,"hotspot",hot.noise(p[0],p[1]),true); n(b,"height",height.noise(p[0],p[1]),false); b.append('}');
            }
        }
        b.append("\n  ],\n  \"constant_hotspot_branch_samples\":[\n"); first = true;
        for (long seed : SEEDS) for (double threshold : THRESHOLDS) {
            double[] values = {Math.nextDown(threshold), threshold, Math.nextUp(threshold)};
            for (int side = 0; side < values.length; side++) {
                final double value = values[side];
                Noise2D height = TFGBiomeNoise.dormantShieldVolcano(seed, (x,z) -> value);
                for (int[] p : new int[][] {{-1808,5232},{5504,-6368}}) {
                    if (!first) b.append(",\n"); first = false;
                    b.append("    {"); i(b,"seed",seed,true); i(b,"x",p[0],true); i(b,"z",p[1],true);
                    n(b,"hotspot",value,true); n(b,"threshold",threshold,true); i(b,"side",side-1,true);
                    n(b,"height",height.noise(p[0],p[1]),false); b.append('}');
                }
            }
        }
        b.append("\n  ]\n}\n"); System.out.print(b);
    }
}
