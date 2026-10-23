const GRADIENTS: [f32; 256] = [
    0.130526192220052,
    0.99144486137381,
    0.38268343236509,
    0.923879532511287,
    0.608761429008721,
    0.793353340291235,
    0.793353340291235,
    0.608761429008721,
    0.923879532511287,
    0.38268343236509,
    0.99144486137381,
    0.130526192220051,
    0.99144486137381,
    -0.130526192220051,
    0.923879532511287,
    -0.38268343236509,
    0.793353340291235,
    -0.60876142900872,
    0.608761429008721,
    -0.793353340291235,
    0.38268343236509,
    -0.923879532511287,
    0.130526192220052,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    -0.38268343236509,
    -0.923879532511287,
    -0.608761429008721,
    -0.793353340291235,
    -0.793353340291235,
    -0.608761429008721,
    -0.923879532511287,
    -0.38268343236509,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    0.130526192220051,
    -0.923879532511287,
    0.38268343236509,
    -0.793353340291235,
    0.608761429008721,
    -0.608761429008721,
    0.793353340291235,
    -0.38268343236509,
    0.923879532511287,
    -0.130526192220052,
    0.99144486137381,
    0.130526192220052,
    0.99144486137381,
    0.38268343236509,
    0.923879532511287,
    0.608761429008721,
    0.793353340291235,
    0.793353340291235,
    0.608761429008721,
    0.923879532511287,
    0.38268343236509,
    0.99144486137381,
    0.130526192220051,
    0.99144486137381,
    -0.130526192220051,
    0.923879532511287,
    -0.38268343236509,
    0.793353340291235,
    -0.60876142900872,
    0.608761429008721,
    -0.793353340291235,
    0.38268343236509,
    -0.923879532511287,
    0.130526192220052,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    -0.38268343236509,
    -0.923879532511287,
    -0.608761429008721,
    -0.793353340291235,
    -0.793353340291235,
    -0.608761429008721,
    -0.923879532511287,
    -0.38268343236509,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    0.130526192220051,
    -0.923879532511287,
    0.38268343236509,
    -0.793353340291235,
    0.608761429008721,
    -0.608761429008721,
    0.793353340291235,
    -0.38268343236509,
    0.923879532511287,
    -0.130526192220052,
    0.99144486137381,
    0.130526192220052,
    0.99144486137381,
    0.38268343236509,
    0.923879532511287,
    0.608761429008721,
    0.793353340291235,
    0.793353340291235,
    0.608761429008721,
    0.923879532511287,
    0.38268343236509,
    0.99144486137381,
    0.130526192220051,
    0.99144486137381,
    -0.130526192220051,
    0.923879532511287,
    -0.38268343236509,
    0.793353340291235,
    -0.60876142900872,
    0.608761429008721,
    -0.793353340291235,
    0.38268343236509,
    -0.923879532511287,
    0.130526192220052,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    -0.38268343236509,
    -0.923879532511287,
    -0.608761429008721,
    -0.793353340291235,
    -0.793353340291235,
    -0.608761429008721,
    -0.923879532511287,
    -0.38268343236509,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    0.130526192220051,
    -0.923879532511287,
    0.38268343236509,
    -0.793353340291235,
    0.608761429008721,
    -0.608761429008721,
    0.793353340291235,
    -0.38268343236509,
    0.923879532511287,
    -0.130526192220052,
    0.99144486137381,
    0.130526192220052,
    0.99144486137381,
    0.38268343236509,
    0.923879532511287,
    0.608761429008721,
    0.793353340291235,
    0.793353340291235,
    0.608761429008721,
    0.923879532511287,
    0.38268343236509,
    0.99144486137381,
    0.130526192220051,
    0.99144486137381,
    -0.130526192220051,
    0.923879532511287,
    -0.38268343236509,
    0.793353340291235,
    -0.60876142900872,
    0.608761429008721,
    -0.793353340291235,
    0.38268343236509,
    -0.923879532511287,
    0.130526192220052,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    -0.38268343236509,
    -0.923879532511287,
    -0.608761429008721,
    -0.793353340291235,
    -0.793353340291235,
    -0.608761429008721,
    -0.923879532511287,
    -0.38268343236509,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    0.130526192220051,
    -0.923879532511287,
    0.38268343236509,
    -0.793353340291235,
    0.608761429008721,
    -0.608761429008721,
    0.793353340291235,
    -0.38268343236509,
    0.923879532511287,
    -0.130526192220052,
    0.99144486137381,
    0.130526192220052,
    0.99144486137381,
    0.38268343236509,
    0.923879532511287,
    0.608761429008721,
    0.793353340291235,
    0.793353340291235,
    0.608761429008721,
    0.923879532511287,
    0.38268343236509,
    0.99144486137381,
    0.130526192220051,
    0.99144486137381,
    -0.130526192220051,
    0.923879532511287,
    -0.38268343236509,
    0.793353340291235,
    -0.60876142900872,
    0.608761429008721,
    -0.793353340291235,
    0.38268343236509,
    -0.923879532511287,
    0.130526192220052,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    -0.38268343236509,
    -0.923879532511287,
    -0.608761429008721,
    -0.793353340291235,
    -0.793353340291235,
    -0.608761429008721,
    -0.923879532511287,
    -0.38268343236509,
    -0.99144486137381,
    -0.130526192220052,
    -0.99144486137381,
    0.130526192220051,
    -0.923879532511287,
    0.38268343236509,
    -0.793353340291235,
    0.608761429008721,
    -0.608761429008721,
    0.793353340291235,
    -0.38268343236509,
    0.923879532511287,
    -0.130526192220052,
    0.99144486137381,
    0.38268343236509,
    0.923879532511287,
    0.923879532511287,
    0.38268343236509,
    0.923879532511287,
    -0.38268343236509,
    0.38268343236509,
    -0.923879532511287,
    -0.38268343236509,
    -0.923879532511287,
    -0.923879532511287,
    -0.38268343236509,
    -0.923879532511287,
    0.38268343236509,
    -0.38268343236509,
    0.923879532511287,
];

const RAND_VECS: [f32; 512] = [
    -0.2700222198,
    -0.9628540911,
    0.3863092627,
    -0.9223693152,
    0.04444859006,
    -0.999011673,
    -0.5992523158,
    -0.8005602176,
    -0.7819280288,
    0.6233687174,
    0.9464672271,
    0.3227999196,
    -0.6514146797,
    -0.7587218957,
    0.9378472289,
    0.347048376,
    -0.8497875957,
    -0.5271252623,
    -0.879042592,
    0.4767432447,
    -0.892300288,
    -0.4514423508,
    -0.379844434,
    -0.9250503802,
    -0.9951650832,
    0.0982163789,
    0.7724397808,
    -0.6350880136,
    0.7573283322,
    -0.6530343002,
    -0.9928004525,
    -0.119780055,
    -0.0532665713,
    0.9985803285,
    0.9754253726,
    -0.2203300762,
    -0.7665018163,
    0.6422421394,
    0.991636706,
    0.1290606184,
    -0.994696838,
    0.1028503788,
    -0.5379205513,
    -0.84299554,
    0.5022815471,
    -0.8647041387,
    0.4559821461,
    -0.8899889226,
    -0.8659131224,
    -0.5001944266,
    0.0879458407,
    -0.9961252577,
    -0.5051684983,
    0.8630207346,
    0.7753185226,
    -0.6315704146,
    -0.6921944612,
    0.7217110418,
    -0.5191659449,
    -0.8546734591,
    0.8978622882,
    -0.4402764035,
    -0.1706774107,
    0.9853269617,
    -0.9353430106,
    -0.3537420705,
    -0.9992404798,
    0.03896746794,
    -0.2882064021,
    -0.9575683108,
    -0.9663811329,
    0.2571137995,
    -0.8759714238,
    -0.4823630009,
    -0.8303123018,
    -0.5572983775,
    0.05110133755,
    -0.9986934731,
    -0.8558373281,
    -0.5172450752,
    0.09887025282,
    0.9951003332,
    0.9189016087,
    0.3944867976,
    -0.2439375892,
    -0.9697909324,
    -0.8121409387,
    -0.5834613061,
    -0.9910431363,
    0.1335421355,
    0.8492423985,
    -0.5280031709,
    -0.9717838994,
    -0.2358729591,
    0.9949457207,
    0.1004142068,
    0.6241065508,
    -0.7813392434,
    0.662910307,
    0.7486988212,
    -0.7197418176,
    0.6942418282,
    -0.8143370775,
    -0.5803922158,
    0.104521054,
    -0.9945226741,
    -0.1065926113,
    -0.9943027784,
    0.445799684,
    -0.8951327509,
    0.105547406,
    0.9944142724,
    -0.992790267,
    0.1198644477,
    -0.8334366408,
    0.552615025,
    0.9115561563,
    -0.4111755999,
    0.8285544909,
    -0.5599084351,
    0.7217097654,
    -0.6921957921,
    0.4940492677,
    -0.8694339084,
    -0.3652321272,
    -0.9309164803,
    -0.9696606758,
    0.2444548501,
    0.08925509731,
    -0.996008799,
    0.5354071276,
    -0.8445941083,
    -0.1053576186,
    0.9944343981,
    -0.9890284586,
    0.1477251101,
    0.004856104961,
    0.9999882091,
    0.9885598478,
    0.1508291331,
    0.9286129562,
    -0.3710498316,
    -0.5832393863,
    -0.8123003252,
    0.3015207509,
    0.9534596146,
    -0.9575110528,
    0.2883965738,
    0.9715802154,
    -0.2367105511,
    0.229981792,
    0.9731949318,
    0.955763816,
    -0.2941352207,
    0.740956116,
    0.6715534485,
    -0.9971513787,
    -0.07542630764,
    0.6905710663,
    -0.7232645452,
    -0.290713703,
    -0.9568100872,
    0.5912777791,
    -0.8064679708,
    -0.9454592212,
    -0.325740481,
    0.6664455681,
    0.74555369,
    0.6236134912,
    0.7817328275,
    0.9126993851,
    -0.4086316587,
    -0.8191762011,
    0.5735419353,
    -0.8812745759,
    -0.4726046147,
    0.9953313627,
    0.09651672651,
    0.9855650846,
    -0.1692969699,
    -0.8495980887,
    0.5274306472,
    0.6174853946,
    -0.7865823463,
    0.8508156371,
    0.52546432,
    0.9985032451,
    -0.05469249926,
    0.1971371563,
    -0.9803759185,
    0.6607855748,
    -0.7505747292,
    -0.03097494063,
    0.9995201614,
    -0.6731660801,
    0.739491331,
    -0.7195018362,
    -0.6944905383,
    0.9727511689,
    0.2318515979,
    0.9997059088,
    -0.0242506907,
    0.4421787429,
    -0.8969269532,
    0.9981350961,
    -0.061043673,
    -0.9173660799,
    -0.3980445648,
    -0.8150056635,
    -0.5794529907,
    -0.8789331304,
    0.4769450202,
    0.0158605829,
    0.999874213,
    -0.8095464474,
    0.5870558317,
    -0.9165898907,
    -0.3998286786,
    -0.8023542565,
    0.5968480938,
    -0.5176737917,
    0.8555780767,
    -0.8154407307,
    -0.5788405779,
    0.4022010347,
    -0.9155513791,
    -0.9052556868,
    -0.4248672045,
    0.7317445619,
    0.6815789728,
    -0.5647632201,
    -0.8252529947,
    -0.8403276335,
    -0.5420788397,
    -0.9314281527,
    0.363925262,
    0.5238198472,
    0.8518290719,
    0.7432803869,
    -0.6689800195,
    -0.985371561,
    -0.1704197369,
    0.4601468731,
    0.88784281,
    0.825855404,
    0.5638819483,
    0.6182366099,
    0.7859920446,
    0.8331502863,
    -0.553046653,
    0.1500307506,
    0.9886813308,
    -0.662330369,
    -0.7492119075,
    -0.668598664,
    0.743623444,
    0.7025606278,
    0.7116238924,
    -0.5419389763,
    -0.8404178401,
    -0.3388616456,
    0.9408362159,
    0.8331530315,
    0.5530425174,
    -0.2989720662,
    -0.9542618632,
    0.2638522993,
    0.9645630949,
    0.124108739,
    -0.9922686234,
    -0.7282649308,
    -0.6852956957,
    0.6962500149,
    0.7177993569,
    -0.9183535368,
    0.3957610156,
    -0.6326102274,
    -0.7744703352,
    -0.9331891859,
    -0.359385508,
    -0.1153779357,
    -0.9933216659,
    0.9514974788,
    -0.3076565421,
    -0.08987977445,
    -0.9959526224,
    0.6678496916,
    0.7442961705,
    0.7952400393,
    -0.6062947138,
    -0.6462007402,
    -0.7631674805,
    -0.2733598753,
    0.9619118351,
    0.9669590226,
    -0.254931851,
    -0.9792894595,
    0.2024651934,
    -0.5369502995,
    -0.8436138784,
    -0.270036471,
    -0.9628500944,
    -0.6400277131,
    0.7683518247,
    -0.7854537493,
    -0.6189203566,
    0.06005905383,
    -0.9981948257,
    -0.02455770378,
    0.9996984141,
    -0.65983623,
    0.751409442,
    -0.6253894466,
    -0.7803127835,
    -0.6210408851,
    -0.7837781695,
    0.8348888491,
    0.5504185768,
    -0.1592275245,
    0.9872419133,
    0.8367622488,
    0.5475663786,
    -0.8675753916,
    -0.4973056806,
    -0.2022662628,
    -0.9793305667,
    0.9399189937,
    0.3413975472,
    0.9877404807,
    -0.1561049093,
    -0.9034455656,
    0.4287028224,
    0.1269804218,
    -0.9919052235,
    -0.3819600854,
    0.924178821,
    0.9754625894,
    0.2201652486,
    -0.3204015856,
    -0.9472818081,
    -0.9874760884,
    0.1577687387,
    0.02535348474,
    -0.9996785487,
    0.4835130794,
    -0.8753371362,
    -0.2850799925,
    -0.9585037287,
    -0.06805516006,
    -0.99768156,
    -0.7885244045,
    -0.6150034663,
    0.3185392127,
    -0.9479096845,
    0.8880043089,
    0.4598351306,
    0.6476921488,
    -0.7619021462,
    0.9820241299,
    0.1887554194,
    0.9357275128,
    -0.3527237187,
    -0.8894895414,
    0.4569555293,
    0.7922791302,
    0.6101588153,
    0.7483818261,
    0.6632681526,
    -0.7288929755,
    -0.6846276581,
    0.8729032783,
    -0.4878932944,
    0.8288345784,
    0.5594937369,
    0.08074567077,
    0.9967347374,
    0.9799148216,
    -0.1994165048,
    -0.580730673,
    -0.8140957471,
    -0.4700049791,
    -0.8826637636,
    0.2409492979,
    0.9705377045,
    0.9437816757,
    -0.3305694308,
    -0.8927998638,
    -0.4504535528,
    -0.8069622304,
    0.5906030467,
    0.06258973166,
    0.9980393407,
    -0.9312597469,
    0.3643559849,
    0.5777449785,
    0.8162173362,
    -0.3360095855,
    -0.941858566,
    0.697932075,
    -0.7161639607,
    -0.002008157227,
    -0.9999979837,
    -0.1827294312,
    -0.9831632392,
    -0.6523911722,
    0.7578824173,
    -0.4302626911,
    -0.9027037258,
    -0.9985126289,
    -0.05452091251,
    -0.01028102172,
    -0.9999471489,
    -0.4946071129,
    0.8691166802,
    -0.2999350194,
    0.9539596344,
    0.8165471961,
    0.5772786819,
    0.2697460475,
    0.962931498,
    -0.7306287391,
    -0.6827749597,
    -0.7590952064,
    -0.6509796216,
    -0.907053853,
    0.4210146171,
    -0.5104861064,
    -0.8598860013,
    0.8613350597,
    0.5080373165,
    0.5007881595,
    -0.8655698812,
    -0.654158152,
    0.7563577938,
    -0.8382755311,
    -0.545246856,
    0.6940070834,
    0.7199681717,
    0.06950936031,
    0.9975812994,
    0.1702942185,
    -0.9853932612,
    0.2695973274,
    0.9629731466,
    0.5519612192,
    -0.8338697815,
    0.225657487,
    -0.9742067022,
    0.4215262855,
    -0.9068161835,
    0.4881873305,
    -0.8727388672,
    -0.3683854996,
    -0.9296731273,
    -0.9825390578,
    0.1860564427,
    0.81256471,
    0.5828709909,
    0.3196460933,
    -0.9475370046,
    0.9570913859,
    0.2897862643,
    -0.6876655497,
    -0.7260276109,
    -0.9988770922,
    -0.047376731,
    -0.1250179027,
    0.992154486,
    -0.8280133617,
    0.560708367,
    0.9324863769,
    -0.3612051451,
    0.6394653183,
    0.7688199442,
    -0.01623847064,
    -0.9998681473,
    -0.9955014666,
    -0.09474613458,
    -0.81453315,
    0.580117012,
    0.4037327978,
    -0.9148769469,
    0.9944263371,
    0.1054336766,
    -0.1624711654,
    0.9867132919,
    -0.9949487814,
    -0.100383875,
    -0.6995302564,
    0.7146029809,
    0.5263414922,
    -0.85027327,
    -0.5395221479,
    0.841971408,
    0.6579370318,
    0.7530729462,
    0.01426758847,
    -0.9998982128,
    -0.6734383991,
    0.7392433447,
    0.639412098,
    -0.7688642071,
    0.9211571421,
    0.3891908523,
    -0.146637214,
    -0.9891903394,
    -0.782318098,
    0.6228791163,
    -0.5039610839,
    -0.8637263605,
    -0.7743120191,
    -0.6328039957,
];

/// TFC/TFG two-dimensional deterministic noise primitives, ported with Java float boundaries.
use std::rc::Rc;

pub type Noise2D = Rc<dyn Fn(f64, f64) -> f64>;
const PRIME_X: i32 = 501125321;
const PRIME_Y: i32 = 1136930381;
#[inline]
fn f<T: Into<f64>>(v: T) -> f32 {
    v.into() as f32
}
#[inline]
pub fn fast_floor(v: f64) -> i32 {
    if v >= 0.0 { v as i32 } else { (v as i32) - 1 }
}
#[inline]
fn hash2d(seed: i32, x: i32, y: i32) -> i32 {
    (seed ^ x ^ y).wrapping_mul(0x27d4eb2d)
}
#[inline]
fn grad(seed: i32, x: i32, y: i32, dx: f32, dy: f32) -> f32 {
    let mut h = hash2d(seed, x, y);
    h ^= h >> 15;
    let i = (h & 254) as usize;
    f(f(dx * GRADIENTS[i]) + f(dy * GRADIENTS[i | 1]))
}

#[derive(Clone, Debug)]
pub struct OpenSimplex2D {
    seed: i32,
    frequency: f64,
    midpoint: f64,
    amplitude: f64,
    octaves: u32,
}
impl OpenSimplex2D {
    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ (seed >> 32)) as i32,
            frequency: 1.,
            midpoint: 0.,
            amplitude: 1.,
            octaves: 1,
        }
    }
    /// Java overload for an `int` seed, which FastNoiseLite stores without folding.
    pub fn from_int(seed: i32) -> Self {
        Self {
            seed,
            frequency: 1.0,
            midpoint: 0.0,
            amplitude: 1.0,
            octaves: 1,
        }
    }
    pub fn spread(&mut self, scale: f64) -> &mut Self {
        self.frequency *= scale;
        self
    }
    pub fn scaled(&mut self, min: f64, max: f64) -> &mut Self {
        self.midpoint = (max + min) / 2.;
        self.amplitude = (max - min) / 2.;
        self
    }
    pub fn octaves(&mut self, n: u32) -> &mut Self {
        self.octaves = n.max(1);
        self.frequency *= 1.0 / ((1u32 << (self.octaves - 1).min(30)) as f64);
        self
    }
    pub fn sample(&self, x: f64, z: f64) -> f64 {
        self.midpoint
            + open_simplex(self.seed, self.frequency, self.octaves, x, z) as f64 * self.amplitude
    }
    pub fn as_noise(&self) -> Noise2D {
        let copy = self.clone();
        Rc::new(move |x, z| copy.sample(x, z))
    }
    pub fn warp(&self, x: f64, z: f64) -> (f64, f64) {
        let mut c = (x, z);
        domain_warp_independent(
            &mut c,
            self.seed,
            self.amplitude * 2.0,
            self.frequency,
            self.octaves,
        );
        c
    }
}
fn simplex_single(seed: i32, x: f64, y: f64) -> f32 {
    const G2: f64 = (3.0 - 1.7320508075688772) / 6.0;
    let mut i = fast_floor(x);
    let mut j = fast_floor(y);
    let xi = f(x - i as f64);
    let yi = f(y - j as f64);
    i = i.wrapping_mul(PRIME_X);
    j = j.wrapping_mul(PRIME_Y);
    let i1 = i.wrapping_add(PRIME_X);
    let j1 = j.wrapping_add(PRIME_Y);
    let g = f(G2 as f64);
    let t = f(f(xi + yi) * g);
    let x0 = f(xi - t);
    let y0 = f(yi - t);
    let mut value = {
        let a = f(f(2.0f32 / 3.0) - f(x0 * x0) - f(y0 * y0));
        let aa = f(a * a);
        f(f(aa * aa) * grad(seed, i, j, x0, y0))
    };
    let a0 = f(f(2.0f32 / 3.0) - f(x0 * x0) - f(y0 * y0));
    let a1_scale = f(2.0 * (1.0 - 2.0 * G2) * (1.0 / G2 - 2.0));
    let a1_offset = f(-2.0 * (1.0 - 2.0 * G2) * (1.0 - 2.0 * G2));
    let a1 = f(f(a1_scale * t) + f(a1_offset + a0));
    let x1 = f(x0 - f((1.0 - 2.0 * G2) as f64));
    let y1 = f(y0 - f((1.0 - 2.0 * G2) as f64));
    let aa = f(a1 * a1);
    value = f(value + f(f(aa * aa) * grad(seed, i1, j1, x1, y1)));
    let xmy = f(xi - yi);
    let mut contribution = |a: f32, hx: i32, hy: i32, ox: f32, oy: f32| {
        if a > 0. {
            let aa = f(a * a);
            value = f(value + f(f(aa * aa) * grad(seed, hx, hy, ox, oy)));
        }
    };
    if t as f64 > G2 {
        if f(xi + xmy) > 1. {
            let ox = f(x0 + f((3.0 * G2 - 2.0) as f64));
            let oy = f(y0 + f((3.0 * G2 - 1.0) as f64));
            let a = f(f(2. / 3.) - f(ox * ox) - f(oy * oy));
            contribution(
                a,
                i.wrapping_add(PRIME_X.wrapping_mul(2)),
                j.wrapping_add(PRIME_Y),
                ox,
                oy,
            );
        } else {
            let ox = f(x0 + g);
            let oy = f(y0 + f((G2 - 1.) as f64));
            let a = f(f(2. / 3.) - f(ox * ox) - f(oy * oy));
            contribution(a, i, j1, ox, oy);
        }
        if f(yi - xmy) > 1. {
            let ox = f(x0 + f((3. * G2 - 1.) as f64));
            let oy = f(y0 + f((3. * G2 - 2.) as f64));
            let a = f(f(2. / 3.) - f(ox * ox) - f(oy * oy));
            contribution(
                a,
                i.wrapping_add(PRIME_X),
                j.wrapping_add(PRIME_Y.wrapping_mul(2)),
                ox,
                oy,
            );
        } else {
            let ox = f(x0 + f((G2 - 1.) as f64));
            let oy = f(y0 + g);
            let a = f(f(2. / 3.) - f(ox * ox) - f(oy * oy));
            contribution(a, i.wrapping_add(PRIME_X), j, ox, oy);
        }
    } else {
        if f(xi + xmy) < 0. {
            let ox = f(x0 + f((1. - G2) as f64));
            let oy = f(y0 - g);
            let a = f(f(2. / 3.) - f(ox * ox) - f(oy * oy));
            contribution(a, i.wrapping_sub(PRIME_X), j, ox, oy);
        } else {
            let ox = f(x0 + f((G2 - 1.) as f64));
            let oy = f(y0 + g);
            let a = f(f(2. / 3.) - f(ox * ox) - f(oy * oy));
            contribution(a, i.wrapping_add(PRIME_X), j, ox, oy);
        }
        if yi < xmy {
            let ox = f(x0 - g);
            let oy = f(y0 - f((G2 - 1.) as f64));
            let a = f(f(2. / 3.) - f(ox * ox) - f(oy * oy));
            contribution(a, i, j.wrapping_sub(PRIME_Y), ox, oy);
        } else {
            let ox = f(x0 + g);
            let oy = f(y0 + f((G2 - 1.) as f64));
            let a = f(f(2. / 3.) - f(ox * ox) - f(oy * oy));
            contribution(a, i, j1, ox, oy);
        }
    }
    f(value * 18.24196194486065f32)
}
fn open_simplex(seed: i32, frequency: f64, octaves: u32, x: f64, z: f64) -> f32 {
    let freq = f(frequency) as f64;
    let mut sx = x * freq;
    let mut sz = z * freq;
    let skew = (sx + sz) * 0.5 * (1.7320508075688772 - 1.);
    sx += skew;
    sz += skew;
    if octaves <= 1 {
        return simplex_single(seed, sx, sz);
    }
    let mut norm = 1f32;
    let mut amp = 0.5f32;
    for _ in 1..octaves {
        norm = f(norm + amp);
        amp = f(amp * 0.5);
    }
    let bound = 1. / norm as f64;
    let (mut s, mut sum, mut amplitude) = (seed, 0f32, bound as f32);
    for _ in 0..octaves {
        let n = simplex_single(s, sx, sz);
        sum = f(sum + f(n * amplitude));
        s = s.wrapping_add(1);
        sx *= 2.;
        sz *= 2.;
        amplitude *= 0.5;
    }
    sum
}

fn fractal_bounding(octaves: u32) -> f32 {
    let mut amp = 0.5f32;
    let mut sum = 1.0f32;
    for _ in 1..octaves {
        sum = f(sum + amp);
        amp = f(amp * 0.5);
    }
    1.0 / sum
}
fn single_domain_warp(
    seed: i32,
    warp_amp: f32,
    frequency: f32,
    x_in: f64,
    y_in: f64,
    coord: &mut (f64, f64),
) {
    const SQRT3: f64 = 1.7320508075688772;
    let g2 = f((3.0f32 - f(SQRT3)) / 6.0f32);
    let x = x_in * frequency as f64;
    let y = y_in * frequency as f64;
    let mut i = fast_floor(x);
    let mut j = fast_floor(y);
    let xi = f(x - i as f64);
    let yi = f(y - j as f64);
    let t = f(f(xi + yi) * g2);
    let x0 = f(xi - t);
    let y0 = f(yi - t);
    i = i.wrapping_mul(PRIME_X);
    j = j.wrapping_mul(PRIME_Y);
    let (mut vx, mut vy) = (0f32, 0f32);
    let mut contribute = |weight: f32, hx: i32, hy: i32, px: f32, py: f32| {
        let hash = hash2d(seed, hx, hy);
        let index1 = (hash & 254) as usize;
        let index2 = ((hash >> 7) & 510) as usize;
        let xg = f(GRADIENTS[index1] as f64);
        let yg = f(GRADIENTS[index1 | 1] as f64);
        let value = f(f(px * xg) + f(py * yg));
        let xgo = f(RAND_VECS[index2] as f64);
        let ygo = f(RAND_VECS[index2 | 1] as f64);
        vx = f(vx + f(weight * f(value * xgo)));
        vy = f(vy + f(weight * f(value * ygo)));
    };
    let a = f(f(0.5f32 - f(x0 * x0)) - f(y0 * y0));
    if a > 0. {
        let aa = f(a * a);
        contribute(f(aa * aa), i, j, x0, y0);
    }
    let one_minus_two = f(1.0f32 - f(2.0f32 * g2));
    let c_slope = f(f(2.0f32 * one_minus_two) * f(f(1.0f32 / g2) - 2.0));
    let c_offset = f(f(-2.0f32 * one_minus_two) * one_minus_two);
    let c = f(f(c_slope * t) + f(c_offset + a));
    if c > 0. {
        let corner = f(f(2.0f32 * g2) - 1.0);
        let x2 = f(x0 + corner);
        let y2 = f(y0 + corner);
        let cc = f(c * c);
        contribute(
            f(cc * cc),
            i.wrapping_add(PRIME_X),
            j.wrapping_add(PRIME_Y),
            x2,
            y2,
        );
    }
    if y0 > x0 {
        let x1 = f(x0 + g2);
        let y1 = f(y0 + f(g2 - 1.0f32));
        let b = f(f(0.5f32 - f(x1 * x1)) - f(y1 * y1));
        if b > 0. {
            let bb = f(b * b);
            contribute(f(bb * bb), i, j.wrapping_add(PRIME_Y), x1, y1);
        }
    } else {
        let x1 = f(x0 + f(g2 - 1.0f32));
        let y1 = f(y0 + g2);
        let b = f(f(0.5f32 - f(x1 * x1)) - f(y1 * y1));
        if b > 0. {
            let bb = f(b * b);
            contribute(f(bb * bb), i.wrapping_add(PRIME_X), j, x1, y1);
        }
    }
    coord.0 += f(vx * warp_amp) as f64;
    coord.1 += f(vy * warp_amp) as f64;
}
fn domain_warp_independent(
    coord: &mut (f64, f64),
    seed: i32,
    domain_amp: f64,
    frequency: f64,
    octaves: u32,
) {
    const F2: f64 = 0.3660254037844386;
    let t = (coord.0 + coord.1) * F2;
    let xs = coord.0 + t;
    let ys = coord.1 + t;
    let mut s = seed;
    let mut amp = f(f(domain_amp as f32) * fractal_bounding(octaves));
    let mut freq = frequency as f32;
    for _ in 0..octaves {
        single_domain_warp(s, f(amp * 38.283687591552734f32), freq, xs, ys, coord);
        s = s.wrapping_add(1);
        amp = f(amp * 0.5);
        freq = f(freq * 2.0);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Cell {
    pub x: f64,
    pub y: f64,
    pub cx: i32,
    pub cy: i32,
    pub f1: f64,
    pub f2: f64,
    pub noise: f64,
    pub angle: f64,
}
#[derive(Clone, Debug)]
pub struct TfgCellular2D {
    seed: i32,
    jitter: f32,
    sample: i32,
    frequency: f64,
}
impl TfgCellular2D {
    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ (seed as u64 >> 32) as i64) as i32,
            jitter: 0.43701595,
            sample: 1,
            frequency: 1.,
        }
    }
    pub fn spread(&mut self, s: f64) -> &mut Self {
        self.frequency *= s;
        self
    }
    pub fn configured(seed: i64, jitter: f32, sample: i32) -> Self {
        Self {
            seed: (seed ^ ((seed as u64) >> 32) as i64) as i32,
            jitter,
            sample: sample.max(0),
            frequency: 1.0,
        }
    }
    pub fn sample_radius(&mut self, s: i32) -> &mut Self {
        self.sample = s.max(0);
        self
    }
    pub fn cell(&self, x: f64, z: f64) -> Cell {
        let x = x * self.frequency;
        let z = z * self.frequency;
        let xr = fast_floor(x);
        let yr = fast_floor(z);
        let (mut d0, mut d1) = (f64::MAX, f64::MAX);
        let (mut cx, mut cy, mut px, mut py, mut nh) = (0, 0, 0., 0., 0i32);
        let mut xp = xr.wrapping_sub(1).wrapping_mul(PRIME_X);
        let ypbase = yr.wrapping_sub(1).wrapping_mul(PRIME_Y);
        for xi in xr - self.sample..=xr + self.sample {
            let mut yp = ypbase;
            for yi in yr - self.sample..=yr + self.sample {
                let h = hash2d(self.seed, xp, yp);
                let idx = (h & 510) as usize;
                let vx = xi as f64 + f(RAND_VECS[idx]) as f64 * self.jitter as f64;
                let vy = yi as f64 + f(RAND_VECS[idx | 1]) as f64 * self.jitter as f64;
                let dx = vx - x;
                let dy = vy - z;
                let nd = dx * dx + dy * dy;
                d1 = d1.min(nd).max(d0);
                if nd < d0 {
                    d0 = nd;
                    cx = xi;
                    cy = yi;
                    px = vx;
                    py = vy;
                    nh = h;
                }
                yp = yp.wrapping_add(PRIME_Y);
            }
            xp = xp.wrapping_add(PRIME_X);
        }
        let dx = px - x;
        let dy = py - z;
        Cell {
            x: px / self.frequency,
            y: py / self.frequency,
            cx,
            cy,
            f1: d0,
            f2: d1,
            noise: f(nh as f64 * (1.0 / 2147483648.0)) as f64,
            angle: diamond_angle(dx, dy),
        }
    }
}
pub fn diamond_angle(x: f64, y: f64) -> f64 {
    if y >= 0. {
        if x >= 0. {
            y / (x + y)
        } else {
            1. - x / (-x + y)
        }
    } else if x < 0. {
        2. - y / (-x - y)
    } else {
        3. + x / (x - y)
    }
}

pub fn scaled(n: Noise2D, min: f64, max: f64) -> Noise2D {
    let scale = (max - min) / 2.;
    let shift = (max + min) / 2.;
    Rc::new(move |x, z| n(x, z) * scale + shift)
}
pub fn add(a: Noise2D, b: Noise2D) -> Noise2D {
    Rc::new(move |x, z| a(x, z) + b(x, z))
}
pub fn map(n: Noise2D, fun: impl Fn(f64) -> f64 + 'static) -> Noise2D {
    Rc::new(move |x, z| fun(n(x, z)))
}
pub fn lazy_product(a: Noise2D, b: Noise2D) -> Noise2D {
    Rc::new(move |x, z| {
        let v = a(x, z);
        if v == 0. { 0. } else { v * b(x, z) }
    })
}
pub fn clamped(n: Noise2D, min: f64, max: f64) -> Noise2D {
    Rc::new(move |x, z| n(x, z).clamp(min, max))
}
pub fn ridged(n: Noise2D) -> Noise2D {
    Rc::new(move |x, z| 1. - 2. * n(x, z).abs())
}
pub fn octaves(n: Noise2D, count: u32) -> Noise2D {
    Rc::new(move |x, z| {
        (0..count)
            .map(|i| {
                n(
                    x / (1u32 << i.min(30)) as f64,
                    z / (1u32 << i.min(30)) as f64,
                ) * 0.5f64.powi((count - i) as i32)
            })
            .sum()
    })
}
pub fn stretch_x(n: Noise2D, s: f64) -> Noise2D {
    Rc::new(move |x, z| n(x / s, z))
}
pub fn stretch_z(n: Noise2D, s: f64) -> Noise2D {
    Rc::new(move |x, z| n(x, z / s))
}
pub fn clamped_scaled(n: Noise2D, old_min: f64, old_max: f64, min: f64, max: f64) -> Noise2D {
    let k = (max - min) / (old_max - old_min);
    let b = min - old_min * k;
    Rc::new(move |x, z| (n(x, z) * k + b).clamp(min, max))
}
pub fn triangle(amplitude: f64, midpoint: f64, frequency: f64, value: f64) -> f64 {
    midpoint
        + amplitude
            * ((4. * frequency * value + 1. - 4. * (frequency * value + 0.75).floor()).abs() - 1.)
}

pub fn abs(n: Noise2D) -> Noise2D {
    Rc::new(move |x, z| n(x, z).abs())
}
pub fn min(a: Noise2D, b: Noise2D) -> Noise2D {
    Rc::new(move |x, z| a(x, z).min(b(x, z)))
}
pub fn max(a: Noise2D, b: Noise2D) -> Noise2D {
    Rc::new(move |x, z| a(x, z).max(b(x, z)))
}
pub fn add_constant(n: Noise2D, v: f64) -> Noise2D {
    Rc::new(move |x, z| n(x, z) + v)
}
pub fn cliff_map(base: Noise2D, compare: Noise2D, addend: Noise2D) -> Noise2D {
    Rc::new(move |x, z| {
        let n = base(x, z);
        if n > compare(x, z) {
            n + addend(x, z)
        } else {
            n
        }
    })
}
pub fn sloped_cliff_map(
    base: Noise2D,
    compare: Noise2D,
    addend: Noise2D,
    slope: Noise2D,
) -> Noise2D {
    Rc::new(move |x, z| {
        let n = base(x, z);
        let c = compare(x, z);
        let a = addend(x, z);
        if n > c + a {
            n + a
        } else if n > c {
            n + ((n - c) * slope(x, z)).min(a)
        } else {
            n
        }
    })
}
pub fn simplex(seed: i64, octave_count: u32, spread: f64) -> Noise2D {
    let mut n = OpenSimplex2D::new(seed);
    n.octaves(octave_count).spread(spread);
    n.as_noise()
}
pub fn scaled_range(n: Noise2D, old_min: f64, old_max: f64, min: f64, max: f64) -> Noise2D {
    let scale = (max - min) / (old_max - old_min);
    let shift = min - old_min * scale;
    Rc::new(move |x, z| n(x, z) * scale + shift)
}
pub fn warped(base: Noise2D, warp: OpenSimplex2D) -> Noise2D {
    Rc::new(move |x, z| {
        let (wx, wz) = warp.warp(x, z);
        base(wx, wz)
    })
}

/// Fluent wrapper around TFC's `Noise2D` defaults.
#[derive(Clone)]
pub struct NoiseChain {
    noise: Noise2D,
}
impl NoiseChain {
    pub fn new(noise: Noise2D) -> Self {
        Self { noise }
    }
    pub fn sample(&self, x: f64, z: f64) -> f64 {
        (self.noise)(x, z)
    }
    pub fn noise(&self) -> Noise2D {
        self.noise.clone()
    }
    pub fn map(&self, fun: impl Fn(f64) -> f64 + 'static) -> Self {
        let n = self.noise.clone();
        Self::new(Rc::new(move |x, z| fun(n(x, z))))
    }
    pub fn scaled(&self, min: f64, max: f64) -> Self {
        Self::new(scaled(self.noise.clone(), min, max))
    }
    pub fn scaled_range(&self, old_min: f64, old_max: f64, min: f64, max: f64) -> Self {
        Self::new(scaled_range(self.noise.clone(), old_min, old_max, min, max))
    }
    pub fn clamped(&self, min: f64, max: f64) -> Self {
        Self::new(clamped(self.noise.clone(), min, max))
    }
    pub fn spread(&self, s: f64) -> Self {
        let n = self.noise.clone();
        Self::new(Rc::new(move |x, z| n(x * s, z * s)))
    }
    pub fn octaves(&self, n: u32) -> Self {
        Self::new(octaves(self.noise.clone(), n))
    }
    pub fn add(&self, other: Noise2D) -> Self {
        Self::new(add(self.noise.clone(), other))
    }
    pub fn lazy_product(&self, other: Noise2D) -> Self {
        Self::new(lazy_product(self.noise.clone(), other))
    }
    pub fn min(&self, other: Noise2D) -> Self {
        Self::new(min(self.noise.clone(), other))
    }
    pub fn max(&self, other: Noise2D) -> Self {
        Self::new(max(self.noise.clone(), other))
    }
    pub fn abs(&self) -> Self {
        Self::new(abs(self.noise.clone()))
    }
    pub fn ridged(&self) -> Self {
        Self::new(ridged(self.noise.clone()))
    }
    pub fn warped(&self, warp: OpenSimplex2D) -> Self {
        Self::new(warped(self.noise.clone(), warp))
    }
    pub fn terraces(&self, levels: i32) -> Self {
        let n = self.noise.clone();
        Self::new(Rc::new(move |x, z| {
            let v = 0.5 * n(x, z) + 0.5;
            ((v * levels as f64) as i32 * 2) as f64 / levels as f64 - 1.
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opensimplex_matches_jdk_fixture_samples() {
        let mut n = OpenSimplex2D::from_int(0);
        n.octaves(1).spread(0.15);
        assert_eq!(n.sample(1.0, -1.0), -0.37110793590545654);
        assert_eq!(n.sample(-1.0, -1.0), -0.5397148132324219);

        let mut n = OpenSimplex2D::from_int(-1);
        n.octaves(4).spread(0.15);
        assert_eq!(n.sample(234375.0, -234375.0), 0.4067649841308594);
    }

    #[test]
    fn tfg_cellular_matches_core_modern_fixture() {
        let n = TfgCellular2D::new(-6696614430994881185);
        let c = n.cell(-3.25, -1.75);
        assert_eq!((c.cx, c.cy), (-3, -2));
        assert_eq!(c.x, -2.565192856882202);
        assert_eq!(c.f1, 0.5114452012978653);
        assert_eq!(c.noise, 0.3067944347858429);
        assert_eq!(c.angle, 3.768647756863094);
    }
}
