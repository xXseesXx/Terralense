// Small independent Java reference for GTCEu 7.5.3 vein selection.
// Run: java tools/reference/fluid_vein_selection.java
// RNG and seed-upgrade equations follow MC 1.20.1 XoroshiroRandomSource and
// RandomSupport; weight vectors are frozen from the installed TFG definitions.
import java.util.Arrays;

class FluidVeinSelectionReference {
    private static final String[] ORDER = {
        "natural_gas_surface_indicator", "light_oil_common", "light_oil_shore",
        "raw_oil_spout_sandstone", "muddy_water", "lava_volcanic",
        "natural_gas_permafrost", "raw_oil_spill_ocean", "oil_common",
        "natural_gas_ocean", "natural_gas_volcanic", "heavy_oil_common",
        "spring_water", "light_oil_spout_hot", "oil_normal", "sea_water",
        "light_oil_spill_ocean", "heavy_oil_spout_dry", "water_common",
        "oil_spill_ocean", "natural_gas_swamp", "raw_oil_spout_hot",
        "heavy_oil_spout_hot", "raw_oil_common", "oil_spout_hot",
        "lava_mountain", "natural_gas_common"
    };

    private static final int[] ACTIVE_SHIELD = {
        0,10,0,0,5,20,0,0,5,0,5,1,20,0,0,0,0,0,0,0,0,0,0,5,0,0,1
    };
    private static final int[] VOLCANIC_MOUNTAIN = {
        0,10,0,0,10,20,0,0,5,0,5,1,20,50,0,0,0,0,10,0,0,30,20,5,30,10,1
    };
    private static final int[] SUNKEN_SHIELD = {
        0,10,0,0,5,20,0,0,5,20,5,1,20,0,0,100,0,0,0,0,0,0,0,5,0,0,1
    };

    private static final class Xoroshiro {
        private long lo;
        private long hi;

        Xoroshiro(long seed) {
            long state = seed ^ 7640891576956012809L;
            lo = mixStafford13(state);
            hi = mixStafford13(state - 7046029254386353131L);
            if ((lo | hi) == 0L) {
                lo = -7046029254386353131L;
                hi = 7640891576956012809L;
            }
        }

        private static long mixStafford13(long value) {
            value = (value ^ (value >>> 30)) * 0xbf58476d1ce4e5b9L;
            value = (value ^ (value >>> 27)) * 0x94d049bb133111ebL;
            return value ^ (value >>> 31);
        }

        int nextInt() {
            long a = lo;
            long b = hi;
            long result = Long.rotateLeft(a + b, 17) + a;
            b ^= a;
            lo = Long.rotateLeft(a, 49) ^ b ^ (b << 21);
            hi = Long.rotateLeft(b, 28);
            return (int) result;
        }
    }

    private static void run(String label, long seed, int cx, int cz, int[] weights) {
        int total = Arrays.stream(weights).sum();
        int veinX = Math.floorDiv(cx, 8);
        int veinZ = Math.floorDiv(cz, 8);
        long packed = Integer.toUnsignedLong(veinX) | (Integer.toUnsignedLong(veinZ) << 32);
        int query = new Xoroshiro(seed ^ packed).nextInt();
        int target = Math.abs(query % total);
        int chosen = -1;
        for (int i = 0; i < weights.length; i++) {
            target -= weights[i];
            if (target < 0) {
                chosen = i;
                break;
            }
        }
        System.out.printf("%s total=%d query=%d initial=%d chosen=%s spring=%s%n",
            label, total, query, Math.abs(query % total), chosen < 0 ? "none" : ORDER[chosen],
            chosen >= 0 && ORDER[chosen].equals("spring_water"));
    }

    public static void main(String[] args) {
        run("active-shield-moderate", 0L, 0, 0, ACTIVE_SHIELD);
        run("volcanic-mountain-hot", 0L, 15, 31, VOLCANIC_MOUNTAIN);
        run("sunken-shield-ocean", 123456789L, 64, -8, SUNKEN_SHIELD);
        run("negative-vein-cell", 0L, -9, -17, VOLCANIC_MOUNTAIN);
    }
}
