//! TFC/TFG surface height factories.
//!
//! The biome-to-factory expressions are bundled directly from the installed TFG profile. A small
//! recursive parser evaluates that source expression against the factory table, preserving nested
//! karst and glacial composition instead of flattening each biome to a guessed height range.
use crate::noise::{self, Noise2D, OpenSimplex2D, TfgCellular2D};
use serde_json::Value;
use std::{rc::Rc, sync::OnceLock};

const HEIGHTS: &str = include_str!("../../assets/data/tfg/biome-heights.json");
const SEA: f64 = 63.0;
static DATA: OnceLock<Value> = OnceLock::new();
fn data() -> &'static Value {
    DATA.get_or_init(|| serde_json::from_str(HEIGHTS).expect("bundled TFG height expressions"))
}

#[derive(Clone, Debug)]
enum Node {
    Number(f64),
    Seed,
    Call(String, Vec<Node>, Vec<(String, Vec<Node>)>),
}
#[derive(Clone)]
enum ValueArg {
    Number(f64),
    Field(Noise2D),
}

fn split_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut d = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => d += 1,
            ')' => d -= 1,
            ',' if d == 0 => {
                out.push(s[start..i].trim());
                start = i + 1
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        out.push(tail)
    }
    out
}
fn matching(s: &str, open: usize) -> Option<usize> {
    let mut d = 0;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        if c == '(' {
            d += 1
        } else if c == ')' {
            d -= 1;
            if d == 0 {
                return Some(i);
            }
        }
    }
    None
}
fn parse(s: &str) -> Result<Node, String> {
    let s = s.trim();
    if s == "seed" {
        return Ok(Node::Seed);
    }
    if let Ok(v) = s
        .trim_end_matches(['f', 'F', 'd', 'D', 'l', 'L'])
        .parse::<f64>()
        && (s.chars().next().unwrap_or(' ').is_ascii_digit() || s.starts_with('-'))
    {
        return Ok(Node::Number(v));
    }
    let open = s
        .find('(')
        .ok_or_else(|| format!("invalid height expression: {s}"))?;
    let close = matching(s, open).ok_or("unbalanced height expression")?;
    let head = s[..open].trim();
    let name = head
        .rsplit(['.', ':'])
        .next()
        .unwrap_or(head)
        .trim()
        .to_owned();
    let args = split_args(&s[open + 1..close])
        .into_iter()
        .map(parse)
        .collect::<Result<Vec<_>, _>>()?;
    let mut chain = Vec::new();
    let mut rest = s[close + 1..].trim();
    while rest.starts_with('.') {
        let op = rest.find('(').ok_or("invalid chained height operation")?;
        let end = matching(rest, op).ok_or("unbalanced chained height operation")?;
        let method = rest[1..op].trim().to_owned();
        let args = split_args(&rest[op + 1..end])
            .into_iter()
            .map(parse)
            .collect::<Result<Vec<_>, _>>()?;
        chain.push((method, args));
        rest = rest[end + 1..].trim();
    }
    if !rest.is_empty() {
        return Err(format!("unsupported height expression tail: {rest}"));
    }
    Ok(Node::Call(name, args, chain))
}

fn number(v: &ValueArg) -> Result<f64, String> {
    match v {
        ValueArg::Number(n) => Ok(*n),
        _ => Err("expected numeric height argument".into()),
    }
}
fn field(v: &ValueArg) -> Result<Noise2D, String> {
    match v {
        ValueArg::Field(n) => Ok(n.clone()),
        _ => Err("expected a nested height field".into()),
    }
}
fn eval(n: &Node, seed: i64) -> Result<ValueArg, String> {
    match n {
        Node::Number(n) => Ok(ValueArg::Number(*n)),
        Node::Seed => Ok(ValueArg::Number(seed as f64)),
        Node::Call(name, args, chain) => {
            let vals = args
                .iter()
                .filter(|x| !matches!(x, Node::Seed))
                .map(|x| eval(x, seed))
                .collect::<Result<Vec<_>, _>>()?;
            let mut result = factory(name, seed, &vals)?;
            for (method, args) in chain {
                let vals = args
                    .iter()
                    .map(|x| eval(x, seed))
                    .collect::<Result<Vec<_>, _>>()?;
                result = chain_field(method, result, &vals)?;
            }
            Ok(ValueArg::Field(result))
        }
    }
}
fn chain_field(method: &str, base: Noise2D, args: &[ValueArg]) -> Result<Noise2D, String> {
    match method {
        "spread" => {
            let scale = number(args.first().ok_or("spread missing scale")?)?;
            Ok(Rc::new(move |x, z| base(x * scale, z * scale)))
        }
        "add" => Ok(noise::add(
            base,
            field(args.first().ok_or("add missing noise")?)?,
        )),
        _ => Err(format!("unsupported height chain .{method}()")),
    }
}

pub fn build_height(seed: i64, biome_name: &str) -> Result<Option<Noise2D>, String> {
    let entry = data()
        .get("biomes")
        .and_then(|biomes| biomes.get(biome_name))
        .ok_or_else(|| format!("no TFG height entry for biome `{biome_name}`"))?;
    let expression = entry
        .get("expression")
        .ok_or_else(|| format!("biome `{biome_name}` has no height expression field"))?;
    if expression.is_null() {
        return Ok(None);
    }
    let expression = expression
        .as_str()
        .ok_or_else(|| format!("height expression for `{biome_name}` is not a string"))?;
    let expr = expression
        .split_once("->")
        .map_or(expression, |(_, rhs)| rhs.trim());
    let node = if let Some((_class, method)) = expr.split_once("::") {
        Node::Call(method.trim().to_owned(), vec![Node::Seed], vec![])
    } else {
        parse(expr).map_err(|e| format!("{biome_name}: {e}"))?
    };
    match eval(&node, seed).map_err(|e| format!("{biome_name}: {e}"))? {
        ValueArg::Field(f) => Ok(Some(f)),
        ValueArg::Number(_) => Err(format!(
            "height expression for `{biome_name}` did not produce a field"
        )),
    }
}

fn factory(name: &str, seed: i64, a: &[ValueArg]) -> Result<Noise2D, String> {
    let n = |i: usize| {
        a.get(i)
            .ok_or_else(|| format!("{name} missing argument {i}"))
            .and_then(number)
    };
    let f = |i: usize| {
        a.get(i)
            .ok_or_else(|| format!("{name} missing field {i}"))
            .and_then(field)
    };
    Ok(match name {
        // TFC 1.20 BiomeNoise
        "hills" => tfc_hills(seed, n(0)?, n(1)?),
        "shore" => tfc_shore(seed),
        "ocean" => tfc_ocean(seed, n(0)?, n(1)?),
        "oceanRidge" => tfc_ocean_ridge(seed, n(0)?, n(1)?),
        "canyons" => tfc_canyons(seed, n(0)?, n(1)?),
        "mountains" => tfc_mountains(seed, n(0)?, n(1)?),
        "lowlands" => tfc_lowlands(seed),
        "lake" => tfc_lake(seed),
        // TFGNoiseHelpers, called both at the outer level and inside compositional expressions.
        "max" => noise::max(f(0)?, f(1)?),
        "min" => noise::min(f(0)?, f(1)?),
        "addConstant" => noise::add_constant(f(0)?, n(1)?),
        // Flat and arid fields.
        "constant" => {
            let y = SEA + n(0)?;
            Rc::new(move |_, _| y)
        }
        "flats" => scaled_simplex(seed, 4, 0.03, SEA - 12., SEA + 8.).pipe_clamp(SEA, SEA + 2.),
        "saltFlats" => {
            scaled_simplex(seed, 4, 0.05, SEA - 16., SEA + 10.).pipe_clamp(SEA - 2., SEA)
        }
        "dunes" => tfg_dunes(seed, n(0)?, n(1)?),
        "sharpHills" => tfg_sharp_hills(seed, n(0)?, n(1)?),
        "badlands" => tfg_badlands(seed, n(0)?, n(1)?),
        "rockyIslands" => tfg_rocky_islands(seed),
        "stairCanyons" => tfg_stepped(seed, 0.05),
        "mesas" => tfg_stepped(seed, 0.12),
        "buttes" => tfg_stepped(seed, 0.18),
        "hoodoos" => tfg_hoodoos(seed),
        // Karst.
        "bowlDolines" => tfg_bowl(seed, f(0)?, n(1)?),
        "fengcong" => tfg_fengcong(seed, f(0)?),
        "fenglin" => tfg_fenglin(seed, f(0)?, n(1)?),
        "cenotes" => tfg_cenotes(seed, f(0)?, n(1)?, n(2)?),
        "burren" => tfg_burren(seed, f(0)?, n(1)?),
        "drumlins" => tfg_drumlins(seed),
        "shilin" => tfg_shilin(seed, f(0)?, n(1)?),
        "tiankeng" => tfg_tiankeng(seed, f(0)?),
        // Glacial and volcanic fields.
        "knobAndKettle" => tfg_knob_kettle(seed),
        "glacialBase" => tfg_glacial_base(seed),
        "iceSheetSurfaceHeight" => tfc_hills(seed, 23., 38.),
        "montaneIceSheetSurfaceHeight" => tfc_hills(seed, 40., 48.),
        "oceanicIceSheetSurfaceHeight" => tfc_hills(seed, 18., 26.),
        "glacialSurfaceTexture" => tfg_glacial_texture(seed),
        "glacialCirques" => tfg_glacial_cirques(seed),
        "glacialCirquesIceSurfaceHeight" => tfg_glacial_ice(seed),
        "patternedGround" => tfg_patterned(seed, false),
        "invertedPatternedGround" => tfg_patterned(seed, true),
        "stoneCircles" => tfg_stone_circles(seed),
        "glaciatedShieldVolcano" => tfg_glaciated_volcano(seed, f(0)?),
        "shieldVolcanoIceSheetSurface" => tfg_shield_ice(seed, f(0)?),
        "shieldVolcanoGlacierSurface" => tfg_shield_glacier(seed, f(0)?),
        "activeShieldVolcano" => tfg_shield(seed, f(0)?, 0),
        "dormantShieldVolcano" => tfg_shield(seed, f(0)?, 1),
        "extinctShieldVolcano" => tfg_shield(seed, f(0)?, 2),
        "ancientShieldVolcano" => tfg_ancient_shield(seed, n(0)?, n(1)?, f(2)?),
        "sunkenShieldVolcano" => tfg_sunken_shield(seed, f(0)?),
        "hotSpotIntensity" | "activeHotSpots" | "dormantHotSpots" | "extinctHotSpots"
        | "ancientHotSpots" => hotspot_factory(seed, name),
        _ => return Err(format!("unsupported TFC/TFG height factory {name}")),
    })
}
trait PipeClamp {
    fn pipe_clamp(self, min: f64, max: f64) -> Noise2D;
}
impl PipeClamp for Noise2D {
    fn pipe_clamp(self, min: f64, max: f64) -> Noise2D {
        noise::clamped(self, min, max)
    }
}
fn simplex(seed: i64, octaves: u32, spread: f64) -> Noise2D {
    let mut n = OpenSimplex2D::new(seed);
    // TFC's `spread` arguments are Java float literals. The TypeScript transcription rounds those
    // at the call site with Math.fround; preserve that boundary before frequency multiplication.
    n.octaves(octaves).spread(spread as f32 as f64);
    n.as_noise()
}
fn scaled_simplex(seed: i64, octaves: u32, spread: f64, min: f64, max: f64) -> Noise2D {
    let mut n = OpenSimplex2D::new(seed);
    n.octaves(octaves)
        .spread(spread as f32 as f64)
        .scaled(min, max);
    n.as_noise()
}
fn tfc_hills(seed: i64, min: f64, max: f64) -> Noise2D {
    scaled_simplex(seed, 4, 0.05, SEA + min, SEA + max)
}
fn tfc_shore(seed: i64) -> Noise2D {
    scaled_simplex(seed, 4, 0.17, SEA, SEA + 5.)
}
fn tfc_ocean(seed: i64, min: f64, max: f64) -> Noise2D {
    let mut warp = OpenSimplex2D::new(seed);
    warp.octaves(2).spread(0.015f32 as f64).scaled(-30., 30.);
    noise::warped(
        scaled_simplex(s(seed, 1), 4, 0.11, SEA + min, SEA + max),
        warp,
    )
}
fn tfc_ocean_ridge(seed: i64, min: f64, max: f64) -> Noise2D {
    let ridge = noise::map(
        noise::ridged(simplex(seed.wrapping_add(1), 4, 0.015)),
        |v| {
            if v <= f32arg(-0.3) {
                0.
            } else {
                let x = (v + f32arg(0.3)) / f32arg(1.3);
                f32arg(-16.) * x * x * x
            }
        },
    );
    let base = scaled_simplex(seed.wrapping_add(2), 4, 0.11, SEA + min, SEA + max);
    let mut warp = OpenSimplex2D::new(seed);
    warp.octaves(2).spread(0.015f32 as f64).scaled(-30., 30.);
    noise::warped(noise::add(base, ridge), warp)
}
fn s(seed: i64, offset: i64) -> i64 {
    seed.wrapping_add(offset)
}
#[inline]
fn f32arg(value: f64) -> f64 {
    value as f32 as f64
}
fn tfc_canyons(seed: i64, min: f64, max: f64) -> Noise2D {
    let mut warp = OpenSimplex2D::new(seed);
    warp.octaves(4).spread(0.03f32 as f64).scaled(-100., 100.);
    let carved = noise::map(noise::warped(simplex(s(seed, 1), 4, 0.06), warp), |v| {
        if v > f32arg(0.4) { v - f32arg(0.8) } else { -v }
    });
    noise::scaled_range(carved, f32arg(-0.4), f32arg(0.8), SEA + min, SEA + max)
}
fn tfc_mountains(seed: i64, base_height: f64, scale_height: f64) -> Noise2D {
    let ridge = noise::ridged(noise::scaled(
        simplex(s(seed, 1), 4, 0.02),
        f32arg(-0.7),
        f32arg(0.7),
    ));
    let base = noise::map(noise::add(simplex(seed, 6, 0.14), ridge), move |x| {
        // Preserve Java/TS left-to-right multiply order; powi(3) is allowed to reassociate.
        let v = 0.125 * (x + 1.) * (x + 1.) * (x + 1.);
        SEA + base_height + scale_height * v
    });
    let cliff = noise::map(
        noise::scaled(simplex(s(seed, 2), 2, 0.01), -25., 25.),
        |x| x.max(0.),
    );
    let height = noise::scaled(simplex(s(seed, 3), 2, 0.01), 120., 160.);
    Rc::new(move |x, z| {
        let mut y = base(x, z);
        if y > 120. {
            let d = height(x, z) - y;
            if d < 0. {
                y += clamped_map(d, 0., -1., 0., 1.) * cliff(x, z)
            }
        }
        y
    })
}
fn tfc_lowlands(seed: i64) -> Noise2D {
    noise::add(
        tfc_hills(seed, -3., -2.),
        noise::clamped(
            noise::scaled(simplex(s(seed, 1), 6, 0.55), -2., 2.),
            -2.,
            1.,
        ),
    )
}
fn tfc_lake(seed: i64) -> Noise2D {
    noise::add(
        scaled_simplex(seed, 4, 0.15, SEA - 12., SEA + 2.),
        noise::clamped(
            noise::scaled(
                noise::map(simplex(s(seed, 1), 5, 0.1), |v| v.powi(4)),
                -2.,
                2.,
            ),
            0.,
            2.,
        ),
    )
}
fn clamped_map(v: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    c + ((v - a) / (b - a)).clamp(0., 1.) * (d - c)
}
fn lerp(delta: f64, start: f64, end: f64) -> f64 {
    start + delta * (end - start)
}

fn tfg_dunes(seed: i64, min: f64, max: f64) -> Noise2D {
    let base = noise::add(
        noise::scaled(simplex(seed, 1, 0.02), -3., 3.),
        Rc::new(|x, z| x / 6. + 20. * (z / 240.).sin()),
    );
    let base = noise::map(base, |v| {
        1.3 * ((v % 5. - 1.).abs() * if v % 5. - v % 1. > 0. { 0.5 } else { 2. } - 1.)
    });
    let base = noise::clamped(base, -1., 1.);
    let moisture = noise::clamped(noise::scaled(simplex(seed, 4, 0.1), -1., 2.), 0.4, 1.);
    noise::scaled(noise::lazy_product(base, moisture), SEA + min, SEA + max)
}
fn sharp_map(v: f64) -> f64 {
    let ranges = [
        (f32arg(0.67), 1.0, f32arg(0.7), 1.0),
        (f32arg(0.15), f32arg(0.67), 0.5, f32arg(0.7)),
        (f32arg(-0.15), f32arg(0.15), -0.5, 0.5),
        (f32arg(-0.67), f32arg(-0.15), f32arg(-0.7), -0.5),
        (-1.0, f32arg(-0.67), -1.0, f32arg(-0.7)),
    ];
    let (a, b, c, d) = ranges
        .into_iter()
        .find(|(a, _, _, _)| v > *a)
        .unwrap_or(ranges[4]);
    c + (v - a) * (d - c) / (b - a)
}
fn tfg_sharp_hills(seed: i64, min: f64, max: f64) -> Noise2D {
    let base = simplex(seed, 4, 0.08);
    let blend = noise::clamped(
        noise::scaled(
            simplex(s(seed, 7198234123), 1, 0.013),
            f32arg(-0.3),
            f32arg(1.6),
        ),
        0.,
        1.,
    );
    let mixed = Rc::new(move |x, z| {
        let v = base(x, z);
        let b = blend(x, z);
        v + b * (sharp_map(v) - v)
    });
    let variance = noise::scaled(
        simplex(s(seed, 67981832123), 3, 0.06),
        f32arg(-0.2),
        f32arg(0.2),
    );
    noise::scaled_range(
        noise::add(mixed, variance),
        f32arg(-0.75),
        f32arg(0.7),
        SEA - min,
        SEA + max,
    )
}
fn tfg_badlands(seed: i64, height: f64, depth: f64) -> Noise2D {
    let base = scaled_simplex(seed, 4, 0.025, SEA + height, SEA + height + 10.);
    let raw = noise::ridged(simplex(s(seed, 1), 4, 0.04));
    let raw = noise::map(raw, |x| {
        f32arg(1.3) * -(if x > 0. { x * x * x } else { 0.5 * x })
    });
    let raw = noise::scaled_range(raw, -1., f32arg(0.3), -1., 1.);
    let terr = noise::map(raw, |v| {
        let t = (0.5 * v + 0.5) * 15.;
        ((t as i32 * 2) as f64 / 15.) - 1.
    });
    let cut = noise::scaled(terr, -depth, 0.);
    let h = noise::add(base, cut);
    noise::map(h, |x| {
        if x < SEA {
            SEA - f32arg(0.3) * (SEA - x)
        } else {
            x
        }
    })
}
fn tfg_rocky_islands(seed: i64) -> Noise2D {
    let base = noise::map(simplex(seed, 4, 0.14), |x| {
        SEA - 15. + 50. * (0.125 * (x + 1.) * (x + 1.) * (x + 1.))
    });
    let cliff = noise::map(
        noise::scaled(simplex(s(seed, 2), 2, 0.01), -10., 18.),
        |x| x.max(0.),
    );
    let cliff_h = noise::scaled(simplex(s(seed, 3), 2, 0.01), SEA - 5., SEA + 5.);
    Rc::new(move |x, z| {
        let mut h = base(x, z);
        if h > SEA - 10. {
            let d = cliff_h(x, z) - h;
            if d < 0. {
                h += clamped_map(d, 0., -1., 0., 1.) * cliff(x, z)
            }
        }
        h
    })
}
fn canyon_base(seed: i64, min: f64, max: f64, width: f64) -> Noise2D {
    noise::clamped_scaled(
        noise::abs(simplex(s(seed, 1), 4, 0.03)),
        width,
        width + 0.28,
        min,
        max,
    )
}
fn tfg_step_cliffs(
    seed: i64,
    input: Noise2D,
    min_start: f64,
    max_start: f64,
    cliff_h: f64,
) -> Noise2D {
    let start = scaled_simplex(s(seed, 3), 2, 0.008, SEA + min_start, SEA + max_start);
    let c1 = noise::clamped(
        noise::scaled(simplex(s(seed, 7), 1, 0.003), -cliff_h, 2. * cliff_h),
        0.,
        cliff_h,
    );
    let double = noise::add(c1.clone(), c1.clone());
    let c2 = noise::clamped(
        noise::scaled(simplex(s(seed, 19), 1, 0.003), -cliff_h, 2. * cliff_h),
        0.,
        cliff_h,
    );
    let c3 = noise::clamped(
        noise::scaled(simplex(s(seed, 25), 1, 0.003), -cliff_h, 2. * cliff_h),
        0.,
        cliff_h,
    );
    let slope_noise = noise::scaled(simplex(s(seed, 33), 1, 0.008), -2., 2.);
    let slope = noise::add(noise::scaled_range(c1.clone(), 0., 7., 3., 6.), slope_noise);
    let second_start = noise::add(start.clone(), double.clone());
    let third_start = noise::add(second_start.clone(), double);
    let first = noise::sloped_cliff_map(input, start, c1, slope.clone());
    let second = noise::sloped_cliff_map(first, second_start, c2, slope.clone());
    noise::sloped_cliff_map(second, third_start, c3, slope)
}
fn tfg_surface(seed: i64, amplitude: f64) -> Noise2D {
    noise::scaled(simplex(seed, 3, 0.08), -amplitude, amplitude)
}
fn tfg_stepped(seed: i64, width: f64) -> Noise2D {
    noise::add(
        tfg_step_cliffs(
            seed,
            canyon_base(seed, SEA + 4., SEA + 22., width),
            5.,
            12.,
            7.,
        ),
        tfg_surface(seed, 5.),
    )
}
fn tfg_hoodoos(seed: i64) -> Noise2D {
    let max = noise::min(
        canyon_base(seed, SEA + 4., SEA + 22., 0.03),
        noise::clamped_scaled(
            noise::abs(simplex(s(seed, 1), 3, 0.12)),
            0.2,
            0.6,
            SEA + 4.,
            SEA + 22.,
        ),
    );
    let shape = noise::max(max, canyon_base(seed, SEA + 4., SEA + 22., 0.2));
    noise::add(
        tfg_step_cliffs(seed, shape, 5., 8., 7.),
        tfg_surface(seed, 3.),
    )
}
fn tfg_bowl(seed: i64, base: Noise2D, scale: f64) -> Noise2D {
    let bowls = noise::map(simplex(seed, 3, 0.72 / scale), move |v| {
        let x = (-0.5 * (std::f64::consts::PI * v).cos() + 0.5).max(0.1) - 0.1;
        -scale * x
    });
    noise::add(base, bowls)
}
fn tfg_fengcong(seed: i64, base: Noise2D) -> Noise2D {
    let cones = noise::map(simplex(seed, 3, 0.06), |v| {
        let y = (-0.5 * (std::f64::consts::PI * v.abs()).cos() + 0.5).max(0.25);
        37. * ((y - 0.25) / 0.75)
    });
    noise::add(base, cones)
}
fn fenglin_cliff(base: Noise2D, compare: Noise2D, addend: Noise2D) -> Noise2D {
    Rc::new(move |x, z| {
        let b = base(x, z);
        if b > compare(x, z) {
            let a = addend(x, z);
            b * (1. - a) + a
        } else {
            b
        }
    })
}
fn tfg_fenglin(seed: i64, base: Noise2D, scale: f64) -> Noise2D {
    let cliffs = noise::scaled(simplex(s(seed, 78535267), 1, 0.06), 0., 0.25);
    let starts = noise::scaled(simplex(s(seed, 390798), 1, 0.06), 0., 0.7);
    let raw = noise::map(simplex(seed, 2, 0.05), |v| {
        let y = v.abs() - 0.45;
        if y > 0. { (y / 0.55).sqrt() } else { 0. }
    });
    noise::add(
        base,
        noise::map(fenglin_cliff(raw, starts, cliffs), move |y| scale * y),
    )
}
fn tfg_cenotes(seed: i64, base: Noise2D, vert: f64, horiz: f64) -> Noise2D {
    let cliffs = noise::scaled(simplex(s(seed, 78535267), 1, 0.72 / horiz), 0., 0.4);
    let starts = noise::scaled(simplex(s(seed, 390798), 1, 0.72 / horiz), 0., 0.7);
    let raw = noise::map(simplex(seed, 2, 0.6 / horiz), |v| {
        let y = v.abs() - 0.45;
        if y > 0. { (y / 0.55).sqrt() } else { 0. }
    });
    noise::add(
        base,
        noise::map(fenglin_cliff(raw, starts, cliffs), move |y| -vert * y),
    )
}
fn tfg_burren(seed: i64, base: Noise2D, scale: f64) -> Noise2D {
    let crevice = noise::map(noise::abs(simplex(s(seed, 398767567), 2, 0.08)), move |y| {
        if y < 0.15 {
            -scale
        } else if y < 0.4 {
            (y - 0.4) * scale
        } else {
            0.
        }
    });
    noise::map(noise::add(crevice, base), |y| y.max(SEA + 2.))
}
fn tfg_drumlins(seed: i64) -> Noise2D {
    let n = scaled_simplex(seed, 3, 0.04, SEA - 16., SEA + 32.);
    noise::stretch_z(n, 2.5)
}
fn tfg_shilin_ridges(seed: i64) -> Noise2D {
    let ridge = noise::map(simplex(s(seed, 398767567), 2, 0.06), |v| {
        let y = v.abs();
        if y < 0.1 {
            1.
        } else if y < 0.2 {
            1. + 0.67 * (y - 0.1) / (0.1 - 0.2)
        } else {
            0.
        }
    });
    let cuts = noise::map(simplex(s(seed, 45764379), 2, 0.03), |v| {
        let y = v.abs();
        let a = if y < 0.065 {
            1.
        } else if y < 0.24 {
            1. + (y - 0.065) / (0.065 - 0.24)
        } else {
            0.
        };
        1. - a
    });
    noise::lazy_product(ridge, cuts)
}
fn tfg_shilin(seed: i64, base: Noise2D, scale: f64) -> Noise2D {
    let bumps = noise::scaled(simplex(s(seed, 83436545633), 1, 0.16), 0.6, 1.);
    let towers = noise::scaled(
        noise::lazy_product(tfg_shilin_ridges(seed), bumps),
        SEA,
        SEA + scale,
    );
    noise::map(noise::max(towers, base), |y| y.max(SEA + 2.))
}
fn tfg_tiankeng(seed: i64, base: Noise2D) -> Noise2D {
    let cliffs = noise::scaled(simplex(s(seed, 78535267), 1, 0.04), 0., 0.04);
    let starts = noise::scaled(simplex(s(seed, 390798), 1, 0.04), 0., 0.7);
    let wide = noise::map(simplex(seed, 2, 0.02), |v| {
        let y = v.abs() - 0.3;
        if y > 0. { (y / 0.7).sqrt() } else { 0. }
    });
    let deep = noise::map(simplex(seed, 2, 0.02), |v| {
        let y = v.abs() - 0.65;
        if y > 0. { (y / 0.35).sqrt() } else { 0. }
    });
    Rc::new(move |x, z| {
        let c = starts(x, z);
        let a = cliffs(x, z);
        let w = wide(x, z);
        let d = deep(x, z);
        let f = |v: f64| if v > c { v * (1. - a) + a } else { v };
        base(x, z) - 22. * f(w) - 24. * f(d)
    })
}
fn tfg_knob_kettle(seed: i64) -> Noise2D {
    let hills = noise::map(simplex(seed, 2, 0.03), |v| {
        if v > 0.3 {
            v - 0.3
        } else if v < -0.3 {
            v + 0.3
        } else {
            0.
        }
    });
    noise::add(noise::scaled(hills, -12., 10.), tfc_hills(seed, -3., 3.))
}
fn tfg_glacial_base(seed: i64) -> Noise2D {
    noise::add_constant(tfg_knob_kettle(seed), 1.5)
}
fn tfg_glacial_valley(seed: i64) -> Noise2D {
    let base = noise::map(simplex(seed, 1, 0.0025), |y| {
        (6. * y * y).min(0.75 + 0.25 * y)
    });
    let other = noise::scaled(simplex(s(seed, 5287), 4, 0.06), -0.2, 0.2);
    noise::add(base, other)
}
fn tfg_glacial_cliff_scale(seed: i64) -> Noise2D {
    noise::clamped(
        noise::scaled(
            noise::add(simplex(s(seed, 78267), 1, 0.015), tfg_glacial_valley(seed)),
            -10.,
            8.,
        ),
        0.,
        7.,
    )
}
fn tfg_glacial_cliff_start(seed: i64) -> Noise2D {
    let val = noise::map(noise::abs(simplex(seed, 1, 0.0025)), |y| {
        if y < 0.43 {
            32. * (1. - y / 0.43)
        } else {
            32. * (y - 0.43) / 0.57
        }
    });
    noise::add(val, tfc_hills(seed, 18., 26.))
}
fn tfg_glacial_ice(seed: i64) -> Noise2D {
    let val = noise::map(noise::abs(simplex(seed, 1, 0.0025)), |y| {
        if y < 0.38 {
            -100.
        } else if y < 0.43 {
            -50. + 50. * (y - 0.38) / 0.05
        } else {
            32. * (y - 0.43) / 0.57
        }
    });
    noise::add(
        val,
        noise::add(tfc_hills(seed, 15., 23.), tfg_glacial_cliff_scale(seed)),
    )
}
fn tfg_glacial_cirques(seed: i64) -> Noise2D {
    let cell_scale = 0.01;
    let shape_center = noise::abs(simplex(seed, 1, 0.0025));
    let shape = tfg_glacial_valley(seed);
    let warp = noise::scaled_range(
        noise::add(simplex(seed, 1, 0.02), shape_center.clone()),
        -1.,
        2.,
        -0.25,
        0.2,
    );
    let rough = noise::scaled(simplex(seed, 3, 0.08), 0.6, 1.6);
    let cliffs = noise::clamped(
        noise::scaled(simplex(s(seed, 785267), 1, 0.01), -12., 15.),
        0.,
        10.,
    );
    let cells = Rc::new({
        let mut c = TfgCellular2D::new(seed);
        c.sample_radius(2).spread(0.01);
        c
    });
    let center_shape = shape_center.clone();
    let body: Noise2D = Rc::new(move |x, z| {
        let c = cells.cell(x, z);
        let diff = if c.f1 > 0. { c.f2 - c.f1 } else { 1. };
        let center = center_shape(c.cx as f64 / cell_scale, c.cy as f64 / cell_scale);
        let w = warp(x, z);
        if center > 0.6 {
            let y = diff + w;
            let r = rough(x, z);
            let scale = lerp(2. * y, 1., r).min(r);
            1. + scale * y
        } else {
            let y = 1. - (diff - w);
            let y = 0.5 * (1. + y * y);
            let point = center_shape(x, z);
            let close = (point - center).min(0.);
            y + clamped_map(diff, 0., 0.1, 0., close)
        }
    });
    let body = noise::add_constant(
        noise::lazy_product(noise::scaled_range(body, 0., 1., 12., 64.), shape),
        SEA - 15.,
    );
    noise::cliff_map(
        noise::cliff_map(body, tfc_hills(seed, 10., 18.), cliffs),
        tfg_glacial_cliff_start(seed),
        tfg_glacial_cliff_scale(seed),
    )
}
fn tfg_patterned(seed: i64, inverted: bool) -> Noise2D {
    let cells = Rc::new(TfgCellular2D::configured(seed, 0.25, 1));
    let mut c = (*cells).clone();
    c.spread(0.05f32 as f64);
    let cells = Rc::new(c);
    if !inverted {
        Rc::new(move |x, z| {
            if {
                let c = cells.cell(x, z);
                c.f2 - c.f1
            } < 0.12
            {
                -1.
            } else {
                0.
            }
        })
    } else {
        let base = tfc_hills(seed, -4., 3.);
        Rc::new(move |x, z| {
            let h = base(x, z);
            let c = cells.cell(x, z);
            let d = c.f2 - c.f1;
            if h >= SEA {
                h + if d < 0.12 { 1. } else { 0. }
            } else if d < 0.12 {
                SEA - 1.
            } else if d < 0.22 {
                SEA - 2.
            } else {
                SEA - 3.
            }
        })
    }
}
fn tfg_stone_circles(seed: i64) -> Noise2D {
    let mut c = TfgCellular2D::configured(seed, 0.26, 1);
    c.spread(0.09f32 as f64);
    let c = Rc::new(c);
    Rc::new(move |x, z| {
        let d = c.cell(x, z).f1;
        if d > 0.06 && d < 0.13 { 1. } else { 0. }
    })
}
fn tfg_glacial_texture(seed: i64) -> Noise2D {
    let warp = noise::scaled(simplex(s(seed, 413), 1, 0.02), -12., 12.);
    Rc::new(move |x, z| {
        let yx = noise::triangle(25., 18., 0.035, x + warp(x, z)).min(0.);
        let yz = noise::triangle(40., 30., 0.025, z + warp(z, x)).min(0.);
        yx.min(yz)
    })
}
fn hotspot_fields(seed: i64) -> (Noise2D, Noise2D, Noise2D, Noise2D) {
    let simplex = OpenSimplex2D::new(seed);
    let simplex = Rc::new(simplex);
    let active: Noise2D = Rc::new(move |x, z| {
        let mut v = 0.;
        for i in 0..3 {
            let a = simplex.sample(x * 0.003 / (1 << i) as f64, z * 0.003 / (1 << i) as f64);
            if a > 0.75 {
                v += (a - 0.75) * 7.2 * 0.5f64.powi(3 - i)
            }
        }
        v
    });
    let u = Rc::new({
        let mut c = TfgCellular2D::new(seed);
        c.spread((0.00590625f32 as f64 / 96.) as f32 as f64);
        c
    });
    let mk = |vel: f64, acc: f64, cut: f64, mul: f64| {
        let active = active.clone();
        let u = u.clone();
        Rc::new(move |x: f64, z: f64| {
            let ux = u.cell(x, z).noise;
            let uz = (((ux * 16.).abs() % 1. > 0.5) as i32 as f64 * 2. - 1.) * ux * 256. % 1.;
            let vx = (ux + if ux > 0. { 1. } else { -1. }) * vel;
            let vz = (uz + if uz > 0. { 1. } else { -1. }) * vel;
            (active(x + vx - vz * acc, z + vz + vx * acc) - cut).max(0.) * mul
        }) as Noise2D
    };
    (
        active.clone(),
        mk(1024., 0., 0.1, 1.111),
        mk(2048., 0.25, 0.2, 1.25),
        mk(3072., 0.5, 0.3, 1.4286),
    )
}
fn hotspot_factory(seed: i64, name: &str) -> Noise2D {
    let (a, b, c, d) = hotspot_fields(seed);
    match name {
        "hotSpotIntensity" => noise::max(noise::max(a.clone(), b), noise::max(c, d)),
        "activeHotSpots" => a,
        "dormantHotSpots" => b,
        "extinctHotSpots" => c,
        _ => d,
    }
}
fn volcano_surface(
    seed: i64,
    spread: f64,
    min: f64,
    max: f64,
    warp_offset: i64,
    surface_offset: i64,
) -> Noise2D {
    let mut warp = OpenSimplex2D::new(s(seed, warp_offset));
    warp.octaves(4).spread(0.03f32 as f64).scaled(-100., 100.);
    let surf = simplex(s(seed, surface_offset), 4, spread);
    let surf = noise::map(noise::warped(surf, warp), |x| {
        if x > 0.4 { x - f32arg(0.8) } else { -x }
    });
    noise::scaled_range(surf, f32arg(-0.4), f32arg(0.8), min, max)
}
fn tfg_shield(seed: i64, hot: Noise2D, age: u8) -> Noise2D {
    let y = noise::map(hot, move |v| {
        if age == 0 {
            if v < 0.75 {
                SEA + 1. + v / 0.75 * 114.
            } else if v < 0.78 {
                SEA + 115. - (v - 0.75) / 0.03 * 25.
            } else {
                SEA + 90. - (v - 0.78) / 0.22 * 30.
            }
        } else if age == 1 {
            if v < 0.45 {
                SEA + 9. + v / 0.45 * 31.
            } else if v < 0.7 {
                SEA + 40. + (v - 0.45) / (0.7 - 0.45) * 30.
            } else if v < 0.73 {
                SEA + 70. - (v - 0.7) / (0.73 - 0.7) * 20.
            } else if v < 0.85 {
                SEA + 50. - (v - 0.73) / (0.85 - 0.73) * 35.
            } else {
                SEA + 15.
            }
        } else if v < 0.4 {
            SEA + 6. + v / 0.4 * 19.
        } else if v < 0.6 {
            SEA + 25. + (v - 0.4) / 0.2 * 30.
        } else if v < 0.62 {
            SEA + 55. - (v - 0.6) / 0.02 * 30.
        } else if v < 0.75 {
            SEA + 25. - (v - 0.62) / 0.13 * 35.
        } else {
            SEA - 10.
        }
    });
    let flow = if age == 0 {
        let lava = noise::ridged(simplex(s(seed, 23891), 1, 1.0));
        let lava = noise::map(lava, |v| if v < 0.45 { 0. } else { 1. });
        Rc::new(move |x, z| lava(x * 0.01, z * 0.01)) as Noise2D
    } else {
        Rc::new(|_, _| 0.) as Noise2D
    };
    let surface = if age == 0 {
        volcano_surface(seed, 0.06_f32 as f64, -8., 8., 0, 1)
    } else if age == 1 {
        volcano_surface(seed, 0.06_f32 as f64, -12., 12., 43, 44)
    } else {
        volcano_surface(seed, 0.06_f32 as f64, -9., 9., 43, 44)
    };
    noise::add(noise::add(y, flow), surface)
}
fn tfg_glaciated_volcano(seed: i64, hot: Noise2D) -> Noise2D {
    let y = noise::map(hot, |v| {
        if v < 0.45 {
            SEA + 15. + v * 55. / 0.45
        } else if v < 0.72 {
            SEA + 70. + (v - 0.45) * 30. / 0.27
        } else if v < 0.74 {
            SEA + 100. + (v - 0.72) * -40. / 0.02
        } else if v < 0.85 {
            SEA + 60. + (v - 0.74) * -10. / 0.11
        } else {
            SEA + 50.
        }
    });
    noise::add(y, volcano_surface(seed, 0.02f32 as f64, -48., 32., 43, 44))
}

fn tfg_shield_ice(seed: i64, hot: Noise2D) -> Noise2D {
    let y = noise::map(hot, |v| if v < 0.9 { v / 0.9 * 51. } else { 51. });
    noise::add(y, tfc_hills(seed, 23., 38.))
}
fn tfg_shield_glacier(seed: i64, hot: Noise2D) -> Noise2D {
    let y = noise::map(hot, |v| {
        if v < 0.4 {
            SEA - 60.
        } else if v < 0.58 {
            SEA - 60. + (v - 0.4) / 0.18 * 135.
        } else if v < 0.72 {
            SEA + 75. + (v - 0.58) / 0.14 * 17.
        } else if v < 0.9 {
            SEA + 92. + (v - 0.72) / 0.18 * 6.
        } else {
            SEA + 98.
        }
    });
    noise::add(y, scaled_simplex(seed, 3, 0.05, -5., 5.))
}
fn tfg_ancient_shield(seed: i64, min: f64, max: f64, hot: Noise2D) -> Noise2D {
    let base = noise::map(hot, |v| {
        if v < 0.15 {
            90.
        } else if v < 0.6 {
            90. + (v - 0.15) / 0.45 * 40.
        } else if v < 0.63 {
            130. - (v - 0.6) / 0.03 * 22.
        } else if v < 0.7 {
            108. - (v - 0.63) / 0.07 * 18.
        } else {
            90.
        }
    });
    let surface = volcano_surface(seed, 0.06, -20., 0., 43, 44);
    let ridges = noise::ridged(simplex(s(seed, 90183), 1, 1.0));
    let spread = Rc::new(move |x, z| ridges(x * 0.01, z * 0.01));
    let valleys = noise::scaled(noise::octaves(spread, 3), max * 2.2, min);
    let scale = noise::scaled(simplex(s(seed, 789913), 2, 0.008), 0.6, 1.);
    noise::add(
        noise::min(noise::lazy_product(base, scale), valleys),
        surface,
    )
}
fn tfg_sunken_shield(seed: i64, hot: Noise2D) -> Noise2D {
    let y = noise::map(hot, |v| {
        if v < 0.25 {
            50.
        } else if v < 0.45 {
            50. + (v - 0.25) / 0.2 * (SEA - 50.)
        } else if v < 0.6 {
            SEA + (v - 0.45) / 0.15 * 32.
        } else if v < 0.62 {
            94. - (v - 0.6) / 0.02 * 14.
        } else if v < 0.75 {
            80. - (v - 0.62) / 0.13 * 28.
        } else {
            52.
        }
    });
    let surface = volcano_surface(seed, 0.06, -6., 6., 43, 44);
    let scale = noise::scaled(simplex(s(seed, 789913), 2, 0.008), 0.45, 1.);
    noise::add(noise::lazy_product(y, scale), surface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_factories_preserve_legacy_typescript_samples() {
        // Generated by the installed TypeScript height registry itself; unlike a finite/range test,
        // these cases catch formula, coordinate scaling, seed-offset, and domain-warp mistakes.
        let golden: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/tfg-height-ts.json"))
                .expect("valid TypeScript height samples");
        let seeds = golden["seeds"].as_array().unwrap();
        let points = golden["points"].as_array().unwrap();
        let values = golden["values"].as_object().unwrap();
        let mut failures = Vec::new();
        let mut compared = 0;
        for (name, samples_by_seed) in values {
            // The legacy TypeScript dormant-shield implementation used glacial
            // constants. Preserve that historical fixture; this one factory is
            // checked against 128 direct installed-Java oracle cases below.
            if name == "dormant_shield_volcano" {
                continue;
            }
            for (seed_index, seed_value) in seeds.iter().enumerate() {
                let seed = seed_value.as_str().unwrap().parse::<i64>().unwrap();
                let field = build_height(seed, name)
                    .unwrap_or_else(|e| panic!("{name}, seed {seed}: {e}"))
                    .unwrap_or_else(|| panic!("unexpected null field for {name}"));
                let samples = samples_by_seed.as_array().unwrap()[seed_index]
                    .as_array()
                    .unwrap();
                for (point_index, point) in points.iter().enumerate() {
                    let x = point[0].as_f64().unwrap();
                    let z = point[1].as_f64().unwrap();
                    let expected = samples[point_index].as_f64().unwrap();
                    let actual = field(x, z);
                    compared += 1;
                    if (actual - expected).abs() > 1e-9 {
                        failures.push(format!(
                            "{name} seed {seed} at ({x}, {z}): expected {expected:.17}, got {actual:.17} (delta {:.3e})",
                            (actual - expected).abs()
                        ));
                    }
                }
            }
        }
        assert_eq!(compared, (values.len() - 1) * seeds.len() * points.len());
        assert!(
            failures.is_empty(),
            "{} height mismatches:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn every_installed_tfg_height_expression_builds_and_samples() {
        let biomes = data().get("biomes").unwrap().as_object().unwrap();
        let mut count = 0;
        for (name, entry) in biomes {
            let Some(expression) = entry.get("expression").and_then(Value::as_str) else {
                continue;
            };
            let field = build_height(7787531943268589495, name)
                .unwrap_or_else(|e| panic!("{e}: {expression}"))
                .unwrap_or_else(|| panic!("non-null height expression unexpectedly null: {name}"));
            for (x, z) in [(-2345.25, -678.5), (0., 0.), (812.75, 4192.0)] {
                let value = field(x, z);
                assert!(
                    value.is_finite(),
                    "nonfinite height for {name} at {x},{z}: {value}"
                );
            }
            count += 1;
        }
        assert_eq!(count, 108);
    }
}

#[cfg(test)]
#[path = "shield_height_oracle_unit.rs"]
mod shield_height_oracle_unit;
