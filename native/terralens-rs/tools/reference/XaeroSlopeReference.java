import java.util.Locale;

/**
 * Independent numeric oracle for Xaero World Map 1.47's default Overworld
 * terrain slope/depth settings. Derived from the installed MapPixel and
 * MapBlock bytecode/decompilation; this reproduces the equations, not their
 * implementation. It emits an FNV-1a digest for a deterministic sweep.
 */
public final class XaeroSlopeReference {
    private static int channel(float c, float multiplier) {
        return Math.max(0, Math.min(255, (int) (c * multiplier)));
    }

    private static int[] pixel(int y, int north, int northwest) {
        int vertical = Math.max(Byte.MIN_VALUE, Math.min(Byte.MAX_VALUE, y - north));
        int diagonal = Math.max(Byte.MIN_VALUE, Math.min(Byte.MAX_VALUE, y - northwest));
        float crossZ = (float) -vertical;
        float cosine = 0.0F;
        if (crossZ < 1.0F) {
            if (vertical == 1 && diagonal == 1) {
                cosine = 1.0F;
            } else {
                float crossX = (float) (vertical - diagonal);
                float cast = 1.0F - crossZ;
                float crossMagnitude = (float) Math.sqrt((double) (crossX * crossX + 1.0F + crossZ * crossZ));
                cosine = (float) ((double) (cast / crossMagnitude) / Math.sqrt(2.0));
            }
        }

        float direct = 0.0F;
        if (cosine == 1.0F) {
            direct = 0.6666667F;
        } else if (cosine > 0.0F) {
            direct = (float) Math.ceil((double) (cosine * 10.0F) ) / 10.0F * 0.6666667F * 0.88388F;
        }
        float white = 0.5F + direct;
        float slope = 1.0F * 0.2F + white;
        float depth = Math.max(0.9F, Math.min(1.0F, (float) y / 63.0F));
        float multiplier = slope * depth;
        return new int[]{channel(83.0F, multiplier), channel(149.0F, multiplier), channel(221.0F, multiplier)};
    }

    private static long addByte(long hash, int value) {
        return (hash ^ (value & 0xffL)) * 0x100000001b3L;
    }

    public static void main(String[] args) {
        Locale.setDefault(Locale.ROOT);
        int[] heights = {-32, 0, 56, 57, 62, 63, 64, 100, 256, 319};
        long hash = 0xcbf29ce484222325L;
        long samples = 0;
        for (int y : heights) {
            for (int vertical = -160; vertical <= 160; vertical++) {
                for (int diagonal = -160; diagonal <= 160; diagonal++) {
                    int[] color = pixel(y, y - vertical, y - diagonal);
                    for (int c : color) hash = addByte(hash, c);
                    samples++;
                }
            }
        }
        System.out.printf("samples=%d fnv1a=%016x%n", samples, hash);
        for (int[] c : new int[][]{{63, 63, 63}, {63, 62, 62}, {63, 64, 63}, {50, 50, 50}, {80, 80, 80}}) {
            int[] result = pixel(c[0], c[1], c[2]);
            System.out.printf("%d,%d,%d => %d,%d,%d%n", c[0], c[1], c[2], result[0], result[1], result[2]);
        }
    }
}
