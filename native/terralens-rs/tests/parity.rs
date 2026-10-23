//! Native accuracy gates. Saved observations are independent Minecraft data; the separate
//! TypeScript snapshot checks conversion parity and is explicitly not a new native oracle.
use serde_json::Value;
use terralens::{
    biome::ids,
    model::Settings,
    random::{JavaRandom, Xoroshiro},
    region::Generator,
};

fn fixture(text: &str) -> Value {
    serde_json::from_str(text).expect("valid immutable fixture")
}
fn integer(v: &Value, key: &str) -> i32 {
    v[key]
        .as_i64()
        .unwrap_or_else(|| panic!("missing integer {key}")) as i32
}
fn seed(v: &Value) -> i64 {
    v["seed"]
        .as_str()
        .expect("seed preserved as decimal string")
        .parse()
        .expect("signed 64-bit seed")
}
fn observations() -> Value {
    fixture(include_str!("fixtures/ganymede-observations.json"))
}
fn reference() -> Value {
    fixture(include_str!("fixtures/tfg-reference.json"))
}

#[derive(Default)]
struct Mismatches {
    count: usize,
    examples: Vec<String>,
    categories: std::collections::BTreeMap<&'static str, usize>,
}
impl Mismatches {
    fn add(&mut self, message: String) {
        self.count += 1;
        let category = if message.contains(" biome (") {
            "biome"
        } else if message.contains("temperature") {
            "temperature"
        } else if message.contains("rainfall") {
            "rainfall"
        } else if message.contains("forest") {
            "forest"
        } else {
            "other"
        };
        *self.categories.entry(category).or_default() += 1;
        if self.examples.len() < 12 {
            self.examples.push(message);
        }
    }
    fn assert_empty(self, label: &str, total: usize) {
        assert_eq!(
            self.count,
            0,
            "{label}: {} mismatches / {total} observations\n{}",
            self.count,
            format!(
                "categories={:?}\n{}",
                self.categories,
                self.examples.join("\n")
            )
        );
    }
}

#[test]
fn installed_ganymede_quart_biomes_match_generated_save() {
    let data = observations();
    let generator = Generator::new(seed(&data), Settings::default());
    let points = data["points"].as_array().unwrap();
    assert_eq!(points.len(), 1556, "fixture must not silently shrink");
    let mut mismatches = Mismatches::default();
    for point in points {
        let (x, z) = (integer(point, "x"), integer(point, "z"));
        let actual = ids::name(generator.biome(x, z));
        let expected = point["biome"].as_str().unwrap();
        if actual != expected {
            mismatches.add(format!("({x}, {z}): expected {expected}, actual {actual}"));
        }
    }
    mismatches.assert_empty("stored quart biome", points.len());
}

#[test]
fn installed_ganymede_climate_matches_generated_chunk_corners() {
    let data = observations();
    let generator = Generator::new(seed(&data), Settings::default());
    let chunks = data["climateChunks"].as_array().unwrap();
    assert_eq!(chunks.len(), 371, "fixture must not silently shrink");
    let mut mismatches = Mismatches::default();
    for point in chunks {
        let (x, z) = (integer(point, "x"), integer(point, "z"));
        let actual = generator.climate(x, z);
        for (key, value) in [
            ("temperature", actual.temperature),
            ("rainfall", actual.rainfall),
        ] {
            let expected = point[key]["00"].as_f64().unwrap();
            // Minecraft stores floats. Compare exact float bits after decimal fixture decoding.
            if (value as f32).to_bits() != (expected as f32).to_bits() {
                mismatches.add(format!(
                    "{key} ({x}, {z}): expected {expected}, actual {value}"
                ));
            }
        }
    }
    mismatches.assert_empty("stored chunk climate", chunks.len() * 2);
}

#[test]
fn installed_ganymede_forest_matches_generated_chunk_capabilities() {
    let data = observations();
    let generator = Generator::new(seed(&data), Settings::default());
    let chunks = data["forestSamples"].as_array().unwrap();
    assert_eq!(chunks.len(), 371, "fixture must not silently shrink");
    let mut mismatches = Mismatches::default();
    for point in chunks {
        let (x, z) = (integer(point, "chunkX"), integer(point, "chunkZ"));
        let actual = generator.forest_type(x, z);
        let expected = integer(point, "forestType") as u8;
        if actual != expected {
            mismatches.add(format!(
                "chunk ({x}, {z}): expected {expected}, actual {actual}"
            ));
        }
    }
    mismatches.assert_empty("stored forest ordinal", chunks.len());
}

#[test]
fn three_seed_biome_climate_forest_conversion_matches_reference() {
    let data = reference();
    let worlds = data["worlds"].as_array().unwrap();
    assert_eq!(worlds.len(), 3);
    let mut mismatches = Mismatches::default();
    let mut total = 0;
    for world in worlds {
        let world_seed = seed(world);
        let generator = Generator::new(world_seed, Settings::default());
        let points = world["points"].as_array().unwrap();
        assert_eq!(points.len(), 274);
        for point in points {
            let (x, z) = (integer(point, "x"), integer(point, "z"));
            let actual_biome = ids::name(generator.biome(x, z));
            let expected_biome = point["biome"].as_str().unwrap();
            if actual_biome != expected_biome {
                mismatches.add(format!("seed {world_seed} biome ({x}, {z}): expected {expected_biome}, actual {actual_biome}"));
            }
            let actual_forest = generator.forest_type(x.div_euclid(16), z.div_euclid(16));
            let expected_forest = integer(point, "forest") as u8;
            if actual_forest != expected_forest {
                mismatches.add(format!("seed {world_seed} forest ({x}, {z}): expected {expected_forest}, actual {actual_forest}"));
            }
            let climate = generator.climate(x, z);
            for (key, actual) in [
                ("temperature", climate.temperature),
                ("rainfall", climate.rainfall),
            ] {
                let expected = point["climate"][key].as_f64().unwrap();
                if (actual as f32).to_bits() != (expected as f32).to_bits() {
                    mismatches.add(format!(
                        "seed {world_seed} {key} ({x}, {z}): expected {expected}, actual {actual}"
                    ));
                }
            }
            total += 4;
        }
    }
    mismatches.assert_empty("three-seed conversion", total);
}

#[test]
fn three_seed_height_rocks_hotspots_match_reference() {
    let data = reference();
    let mut mismatches = Mismatches::default();
    let mut total = 0;
    for world in data["worlds"].as_array().unwrap() {
        let world_seed = seed(world);
        let generator = terralens::world::World::new(world_seed, Settings::default());
        for point in world["points"].as_array().unwrap() {
            let (x, z) = (integer(point, "x"), integer(point, "z"));
            let actual = generator.probe(x, z);
            for (index, key) in ["top", "middle", "bottom"].iter().enumerate() {
                let expected = point["rocks"][key].as_str().unwrap();
                if actual.rocks[index] != expected {
                    mismatches.add(format!(
                        "seed {world_seed} {key} rock ({x}, {z}): expected {expected}, actual {}",
                        actual.rocks[index]
                    ));
                }
            }
            if let Some(expected) = point["height"].as_f64() {
                if actual.height != Some(expected) {
                    mismatches.add(format!(
                        "seed {world_seed} height ({x}, {z}): expected {expected}, actual {:?}",
                        actual.height
                    ));
                }
            } else if actual.height.is_some() {
                mismatches.add(format!("seed {world_seed} height ({x}, {z}): reference unavailable, native fabricated {:?}", actual.height));
            }
            let expected_intensity = point["hotspot"]["intensity"].as_f64().unwrap();
            let expected_age = point["hotspot"]["age"].as_f64().unwrap();
            if (actual.hotspot - expected_intensity).abs() > 1e-12 {
                mismatches.add(format!("seed {world_seed} hotspot ({x}, {z}): expected {expected_intensity}, actual {}", actual.hotspot));
            }
            if actual.hotspot_age != expected_age {
                mismatches.add(format!(
                    "seed {world_seed} hotspot age ({x}, {z}): expected {expected_age}, actual {}",
                    actual.hotspot_age
                ));
            }
            total += 6;
        }
    }
    mismatches.assert_empty("height/geology/hotspot conversion", total);
}

#[test]
fn installed_ganymede_height_keeps_previous_error_bounds() {
    let data = observations();
    let world = terralens::world::World::new(seed(&data), Settings::default());
    let points = data["heightSamples"].as_array().unwrap();
    assert_eq!(points.len(), 371);
    let mut errors: Vec<(f64, i32, i32, f64, f64)> = points
        .iter()
        .map(|point| {
            let height = world
                .height(integer(point, "x"), integer(point, "z"))
                .expect("height unavailable at sampled save location");
            assert!(
                height.is_finite(),
                "height unavailable at ({}, {})",
                point["x"],
                point["z"]
            );
            let expected = point["surfaceY"].as_f64().unwrap();
            (
                (height - expected).abs(),
                integer(point, "x"),
                integer(point, "z"),
                height,
                expected,
            )
        })
        .collect();
    errors.sort_by(|a, b| a.0.total_cmp(&b.0));
    let p95 = errors[((errors.len() - 1) as f64 * 0.95).floor() as usize].0;
    let maximum = errors.last().unwrap().0;
    assert!(
        p95 <= 5.0 && maximum <= 19.0,
        "generated height error regressed: p95={p95}, max={maximum}; worst samples (error,x,z,actual,expected)={:?}; previous validated limits were 5 and 19 blocks",
        errors.iter().rev().take(12).collect::<Vec<_>>()
    );
}

#[test]
fn java_random_matches_real_jdk_fixture() {
    let data = fixture(include_str!("fixtures/core/java-random.json"));
    let cases = data["cases"].as_array().unwrap();
    assert!(cases.len() > 50);
    for case in cases {
        let mut random = JavaRandom::new(seed(case));
        let operation = case["op"].as_str().unwrap();
        for (index, expected) in case["out"].as_array().unwrap().iter().enumerate() {
            let context = format!("{operation} seed {} draw {index}", case["seed"]);
            match operation {
                "nextInt" => assert_eq!(
                    random.next_int() as i64,
                    expected.as_i64().unwrap(),
                    "{context}"
                ),
                "nextIntBound" => assert_eq!(
                    random.next_int_bound(integer(case, "bound")) as i64,
                    expected.as_i64().unwrap(),
                    "{context}"
                ),
                "nextLong" => assert_eq!(
                    random.next_long().to_string(),
                    expected.as_str().unwrap(),
                    "{context}"
                ),
                "nextFloat" => assert_eq!(
                    random.next_float().to_bits(),
                    (expected.as_f64().unwrap() as f32).to_bits(),
                    "{context}"
                ),
                "nextDouble" => assert_eq!(
                    random.next_double(),
                    expected.as_f64().unwrap(),
                    "{context}"
                ),
                "nextBoolean" => assert_eq!(
                    random.next_boolean(),
                    expected.as_bool().unwrap(),
                    "{context}"
                ),
                "nextGaussian" => assert!(
                    (random.next_gaussian() - expected.as_f64().unwrap()).abs() < 1e-12,
                    "{context}"
                ),
                other => panic!("unhandled real JDK fixture operation {other}"),
            }
        }
    }
}

#[test]
fn xoroshiro_streams_match_official_minecraft_fixture() {
    let data = fixture(include_str!("fixtures/core/xoroshiro.json"));
    let mut checked = 0;
    for case in data["cases"].as_array().unwrap() {
        let operation = case["op"].as_str().unwrap();
        if ![
            "nextInt",
            "nextIntBound",
            "nextLong",
            "nextFloat",
            "nextDouble",
            "nextBoolean",
            "nextGaussian",
            "mixed",
            "zeroState",
        ]
        .contains(&operation)
        {
            continue;
        }
        let mut random = if operation == "zeroState" {
            Xoroshiro::from_state(0, 0)
        } else {
            Xoroshiro::new(seed(case))
        };
        for (index, expected) in case["out"].as_array().unwrap().iter().enumerate() {
            let context = format!("{operation} seed {} draw {index}", case["seed"]);
            match operation {
                "nextInt" => assert_eq!(
                    random.next_int() as i64,
                    expected.as_i64().unwrap(),
                    "{context}"
                ),
                "nextIntBound" => assert_eq!(
                    random.next_int_bound(integer(case, "bound")) as i64,
                    expected.as_i64().unwrap(),
                    "{context}"
                ),
                "nextLong" | "zeroState" => assert_eq!(
                    random.next_long().to_string(),
                    expected.as_str().unwrap(),
                    "{context}"
                ),
                "nextFloat" => assert_eq!(
                    random.next_float().to_bits(),
                    (expected.as_f64().unwrap() as f32).to_bits(),
                    "{context}"
                ),
                "nextDouble" => assert_eq!(
                    random.next_double(),
                    expected.as_f64().unwrap(),
                    "{context}"
                ),
                "nextBoolean" => assert_eq!(
                    random.next_boolean(),
                    expected.as_bool().unwrap(),
                    "{context}"
                ),
                "nextGaussian" => assert!(
                    (random.next_gaussian() - expected.as_f64().unwrap()).abs() < 1e-12,
                    "{context}"
                ),
                "mixed" => {
                    random.next_boolean();
                    random.next_double();
                    random.next_int_bound(1_073_741_825);
                    assert_eq!(
                        random.next_long().to_string(),
                        expected.as_str().unwrap(),
                        "{context}"
                    );
                }
                _ => unreachable!(),
            }
            checked += 1;
        }
    }
    assert!(
        checked > 100,
        "official stream fixture coverage unexpectedly shrank"
    );
}
