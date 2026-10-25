import net.dries007.tfc.world.noise.Cellular3D;
import net.dries007.tfc.world.noise.OpenSimplex3D;

/** Calls the installed TFG/TFC noise classes directly; never opens a world. */
public final class SurfaceNoiseOracle {
    public static void main(String[] args) {
        System.out.print("{\"format\":\"tfc-surface-noise-v1\",\"samples\":[");
        boolean first = true;
        for (long seed : new long[]{0, 1, -1, 1234567890123456789L}) {
            var simplex = new OpenSimplex3D(seed).octaves(2).spread(0.1f).scaled(0, 3);
            var cellular = new Cellular3D(seed).spread(0.015f);
            for (int k = 0; k < 128; k++) {
                int x = (k * 7919 % 400001) - 200000;
                int y = (k * 37 % 384) - 64;
                int z = (k * 104729 % 400001) - 200000;
                var cell = cellular.cell(x, (double)(y / 0.6f), z);
                if (!first) System.out.print(','); first = false;
                System.out.print("{\"seed\":"+seed+",\"x\":"+x+",\"y\":"+y+",\"z\":"+z
                    +",\"simplex\":"+simplex.noise(x,y,z)+",\"cell_noise\":"+cell.noise()+",\"cell_y\":"+cell.y()+"}");
            }
        }
        System.out.println("]}");
    }
}
