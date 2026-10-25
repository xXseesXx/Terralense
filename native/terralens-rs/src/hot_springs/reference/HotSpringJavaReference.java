// Standalone Java 17 reference for the exact TFC / Minecraft 1.20.1 RNG path.
// Run: javac -d /tmp/hot-spring-ref src/hot_springs/reference/HotSpringJavaReference.java
//      java -cp /tmp/hot-spring-ref HotSpringJavaReference
//
// The algorithms below mirror Minecraft's XoroshiroRandomSource and legacy
// WorldgenRandom RandomSource methods, then TFC Helpers.seedLargeFeatures,
// Helpers.fork, and Metaballs2D.simple. This intentionally has no Rust or game
// runtime dependency so its output can serve as a regression oracle.

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

public final class HotSpringJavaReference {
    private static final long GOLDEN = 0x9e3779b97f4a7c15L;
    private static final long SILVER = 0x6a09e667f3bcc909L;
    private static final long PRIME_X = 501_125_321L;
    private static final long PRIME_Y = 1_136_930_381L;
    private static final long HASH_MUL = 0x27d4_eb2dL;

    private static long mixStafford13(long z) {
        z = (z ^ (z >>> 30)) * 0xbf58476d1ce4e5b9L;
        z = (z ^ (z >>> 27)) * 0x94d049bb133111ebL;
        return z ^ (z >>> 31);
    }

    private static final class Xoroshiro {
        long lo;
        long hi;

        Xoroshiro(long seed) {
            long loSeed = seed ^ SILVER;
            lo = mixStafford13(loSeed);
            hi = mixStafford13(loSeed + GOLDEN);
            if ((lo | hi) == 0) {
                lo = GOLDEN;
                hi = SILVER;
            }
        }

        Xoroshiro(long lo, long hi) {
            this.lo = lo;
            this.hi = hi;
            if ((lo | hi) == 0) {
                this.lo = GOLDEN;
                this.hi = SILVER;
            }
        }

        long nextLong() {
            long s0 = lo;
            long s1 = hi;
            long result = Long.rotateLeft(s0 + s1, 17) + s0;
            long x = s1 ^ s0;
            lo = Long.rotateLeft(s0, 49) ^ x ^ (x << 21);
            hi = Long.rotateLeft(x, 28);
            return result;
        }

        int nextInt() {
            return (int) nextLong();
        }

        int nextIntBound(int bound) {
            long b = Integer.toUnsignedLong(bound);
            long threshold = Integer.toUnsignedLong(-bound) % b;
            while (true) {
                long product = Integer.toUnsignedLong(nextInt()) * b;
                if ((product & 0xffff_ffffL) >= threshold) {
                    return (int) (product >>> 32);
                }
            }
        }

        int next(int bits) {
            return (int) (nextLong() >>> (64 - bits));
        }

        double nextDouble() {
            return (nextLong() >>> 11) * 0x1.0p-53;
        }
    }

    private static final class WorldgenRandom {
        final Xoroshiro source;

        WorldgenRandom(long seed) {
            source = new Xoroshiro(seed);
        }

        int next(int bits) {
            return source.next(bits);
        }

        long nextLong() {
            return ((long) next(32) << 32) + next(32);
        }

        float nextFloat() {
            return next(24) * 0x1.0p-24f;
        }

        int nextIntBound(int bound) {
            if ((bound & -bound) == bound) {
                return (int) (((long) bound * next(31)) >> 31);
            }
            while (true) {
                int bits = next(31);
                int value = bits % bound;
                if (bits - value + (bound - 1) >= 0) {
                    return value;
                }
            }
        }
    }

    private record Ball(double x, double z, double weight) {}

    private static final class Metaballs {
        final List<Ball> balls = new ArrayList<>();

        Metaballs(WorldgenRandom parent, int size) {
            Xoroshiro fork = new Xoroshiro(parent.nextLong(), parent.nextLong());
            int count = 3 + fork.nextIntBound(5); // Helpers.uniform(3, 8)
            double minSize = (double) (0.1f * size);
            double maxSize = (double) (0.3f * size);
            double radius = (double) (0.5f * size);
            for (int i = 0; i < count; i++) {
                double x = (fork.nextDouble() - fork.nextDouble()) * radius;
                double z = (fork.nextDouble() - fork.nextDouble()) * radius;
                double weight = fork.nextDouble() * (maxSize - minSize) + minSize;
                balls.add(new Ball(x, z, weight));
            }
        }

        boolean inside(int x, int z) {
            double value = 0;
            for (Ball ball : balls) {
                double dx = x - ball.x;
                double dz = z - ball.z;
                value += ball.weight * Math.abs(ball.weight) / (dx * dx + dz * dz);
            }
            return value > 1;
        }
    }

    private static long decorationSeed(int chunkX, int chunkZ, int y) {
        long blockX = (long) chunkX * 16;
        long blockZ = (long) chunkZ * 16;
        long hash = 128_739_412_341L ^ blockX * PRIME_X ^ (long) y * PRIME_Y ^ blockZ;
        return (int) (hash * HASH_MUL);
    }

    private static long featureSeed(int chunkX, int chunkZ, int y, int step, int index) {
        long base = decorationSeed(chunkX, chunkZ, y);
        WorldgenRandom random = new WorldgenRandom(base);
        return (long) index * random.nextLong() * 203_704_237L
                ^ (long) step * random.nextLong() * 758_031_792L
                ^ base;
    }

    private static void emit(int chunkX, int chunkZ) {
        long seed = featureSeed(chunkX, chunkZ, -64, 3, 2);
        WorldgenRandom random = new WorldgenRandom(seed);
        float rarity = random.nextFloat();
        int originX = chunkX * 16 + random.nextIntBound(16);
        int originZ = chunkZ * 16 + random.nextIntBound(16);
        int selector = random.nextIntBound(16);
        Metaballs shape = new Metaballs(random, 14);
        int continuation = random.nextIntBound(1000);
        StringBuilder grid = new StringBuilder();
        for (int z = -14; z <= 14; z++) {
            for (int x = -14; x <= 14; x++) {
                grid.append(shape.inside(x, z) ? '1' : '0');
            }
        }
        System.out.printf(Locale.ROOT,
                "{\"chunk\":[%d,%d],\"step\":3,\"index\":2,\"feature_seed\":%d,"
                        + "\"rarity_float\":%.9g,\"origin\":[%d,%d],\"selector\":%d,"
                        + "\"balls\":[",
                chunkX, chunkZ, seed, rarity, originX, originZ, selector);
        for (int i = 0; i < shape.balls.size(); i++) {
            Ball b = shape.balls.get(i);
            if (i > 0) System.out.print(',');
            System.out.printf(Locale.ROOT, "[%.17g,%.17g,%.17g]", b.x, b.z, b.weight);
        }
        System.out.printf(Locale.ROOT,
                "],\"inside_grid_xz_m14_to_p14\":\"%s\",\"parent_next_int_1000\":%d}%n",
                grid, continuation);
    }

    private record P(int x,int y,int z){}
    private record XZ(int x,int z){}
    // Empty-spring basin pass over independently captured noise heights. This
    // isolates source geometry/RNG; it does not reconstruct live WG heightmaps,
    // preceding decoration, fissures or fluid ticks.
    private static void pool(String file) throws Exception {
        var heights=new java.util.HashMap<XZ,Integer>();
        for(String line:java.nio.file.Files.readAllLines(java.nio.file.Path.of(file))) {
            String[] v=line.split(","); heights.put(new XZ(Integer.parseInt(v[0]),Integer.parseInt(v[1])),Integer.parseInt(v[2]));
        }
        WorldgenRandom r=new WorldgenRandom(featureSeed(-19954,1006,-64,8,30));
        r.nextFloat();int ox=-19954*16+r.nextIntBound(16),oz=1006*16+r.nextIntBound(16);r.nextIntBound(4);
        Metaballs shape=new Metaballs(r,14);
        var air=new java.util.HashSet<P>(); var filled=new java.util.HashSet<P>();
        int count=0;
        for(int dx=-14;dx<=14;dx++)for(int dz=-14;dz<=14;dz++) {
            if(!shape.inside(dx,dz))continue;
            int x=ox+dx,z=oz+dz,y=heights.get(new XZ(x,z));boolean edge=false;
            for(int[] d:new int[][]{{0,-1},{1,0},{0,1},{-1,0}}) {
                P p=new P(x+d[0],y,z+d[1]);
                if((air.contains(p)||heights.get(new XZ(p.x,p.z))<y)&&!filled.contains(p)){edge=true;break;}
            }
            float center=1-0.7f*Math.max(0f,Math.min(1f,2*(dx*dx+dz*dz)/(float)(14*14)));
            int depth=(int)((8+r.nextIntBound(3))*center);
            boolean hole=edge&&r.nextIntBound(12)==0;
            String kind=edge?(hole?"hole":"hardened"):"gravel";
            if(!edge||hole)air.add(new P(x,y,z));if(!edge)filled.add(new P(x,y,z));
            System.out.println(x+","+z+","+(edge&&!hole?y:y-1)+","+kind);
            count++;
        }
        System.err.println("columns="+count+" nextInt1000="+r.nextIntBound(1000));
    }
    public static void main(String[] args) throws Exception {
        if(args.length>0){pool(args[0]);return;}

        // Find a stage3/index2 source whose outer 1/30 filter passes and whose
        // 16-way configured selector chooses a wet spring variant (index 4..15).
        for (int cz = -32; cz <= 32; cz++) {
            for (int cx = -32; cx <= 32; cx++) {
                WorldgenRandom random = new WorldgenRandom(featureSeed(cx, cz, -64, 3, 2));
                if (random.nextFloat() < (1f / 30f)) {
                    random.nextIntBound(16);
                    random.nextIntBound(16);
                    if (random.nextIntBound(16) >= 4) {
                        emit(cx, cz);
                        return;
                    }
                }
            }
        }
        throw new AssertionError("no active spring fixture found");
    }
}
