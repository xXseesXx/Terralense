//! Real child-engine zoom sweep that verifies the rendered viewport, not just queue idleness.
//! Run with `cargo run --release --locked --example zoom_streaming`.
use eframe::egui;
use std::time::{Duration, Instant};
use terralens::{
    atlas::{Atlas, Layer},
    model::Point,
};

struct Runner {
    ctx: egui::Context,
    atlas: Atlas,
    start: Instant,
    size: egui::Vec2,
}
impl Runner {
    fn frame(&mut self, center: Point, bpp: f64, layer: Layer) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            time: Some(self.start.elapsed().as_secs_f64()),
            ..Default::default()
        };
        let _ = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.atlas
                    .paint(ui, ui.available_rect_before_wrap(), center, bpp, layer);
            });
        });
    }
    fn settle(
        &mut self,
        center: Point,
        bpp: f64,
        layer: Layer,
    ) -> (f64, terralens::atlas::TileStreamProgress) {
        let start = Instant::now();
        let mut last_ready = 0;
        let mut stalled_since = Instant::now();
        loop {
            self.frame(center, bpp, layer);
            let progress = self.atlas.tile_stream_progress();
            if progress.fallback_ready > last_ready {
                last_ready = progress.fallback_ready;
                stalled_since = Instant::now();
            }
            if progress.detailed == progress.visible
                && progress.pending == 0
                && progress.uploads == 0
            {
                return (start.elapsed().as_secs_f64() * 1000., progress);
            }
            assert!(
                start.elapsed() < Duration::from_secs(180),
                "zoom sweep timed out at {bpp} blocks/point: {progress:?}; {}",
                self.atlas.status()
            );
            assert!(
                stalled_since.elapsed() < Duration::from_secs(45),
                "visible tiles stopped progressing at {bpp} blocks/point: {progress:?}; {}",
                self.atlas.status()
            );
            std::thread::sleep(Duration::from_millis(4));
        }
    }
}

fn main() {
    if std::env::args().any(|a| a == "--engine") {
        terralens::atlas::run_engine().unwrap();
        return;
    }
    let size = std::env::args().skip(1).collect::<Vec<_>>();
    let size = if size.len() == 2 {
        egui::vec2(size[0].parse().unwrap(), size[1].parse().unwrap())
    } else {
        egui::vec2(3440., 1440.)
    };
    let seed = 7787531943268589495;
    let instance = terralens::prism::default_instance();
    let world = terralens::prism::discover_worlds(&instance)
        .ok()
        .and_then(|worlds| worlds.into_iter().find(|world| world.seed == seed));
    let settings = world
        .as_ref()
        .map(|world| world.settings.clone())
        .unwrap_or_default();
    eprintln!(
        "Using {} settings",
        if world.is_some() {
            "Ganymede"
        } else {
            "default"
        }
    );
    let ctx = egui::Context::default();
    let atlas = Atlas::new(&ctx, seed, settings);
    let mut run = Runner {
        ctx,
        atlas,
        start: Instant::now(),
        size,
    };
    let center = Point {
        x: -2244.,
        z: -2316.,
    };
    // Includes exact powers of two and their geometric midpoints, where rounded tile LOD changes.
    let bpps = [
        1.,
        1.4142135623730951,
        2.,
        2.8284271247461903,
        4.,
        5.656854249492381,
        8.,
        11.313708498984761,
        16.,
        22.627416997969522,
        32.,
    ];
    let mut records = Vec::new();
    for bpp in bpps {
        let (milliseconds, progress) = run.settle(center, bpp, Layer::Biomes);
        assert_eq!(
            progress.holes, 0,
            "fallback coverage must have no holes after settle at {bpp}"
        );
        eprintln!("bpp={bpp:.6}: {milliseconds:.0} ms, {progress:?}");
        records.push(
            serde_json::json!({"blocksPerPoint":bpp,"settleMs":milliseconds,
            "visible":progress.visible,"detailed":progress.detailed,
            "fallbackReady":progress.fallback_ready,"holes":progress.holes}),
        );
    }
    // Stress the LOD boundary while the viewport enters new terrain. This leaves obsolete work
    // queued/in flight, then verifies that the final stationary view recovers completely.
    let mut transition_holes = 0usize;
    let mut transition_max_holes = 0usize;
    for step in 0..120 {
        let bpp = if step % 2 == 0 { 2.82 } else { 2.84 };
        let moved = Point {
            x: center.x + step as f64 * 192.,
            z: center.z - step as f64 * 96.,
        };
        run.frame(moved, bpp, Layer::Biomes);
        let progress = run.atlas.tile_stream_progress();
        transition_holes += progress.holes;
        transition_max_holes = transition_max_holes.max(progress.holes);
        std::thread::sleep(Duration::from_millis(4));
    }
    let final_center = Point {
        x: center.x + 119. * 192.,
        z: center.z - 119. * 96.,
    };
    let (recovery_ms, recovered) = run.settle(final_center, 2.84, Layer::Biomes);
    assert_eq!(recovered.holes, 0);
    assert_eq!(recovered.detailed, recovered.visible);
    records.push(serde_json::json!({
        "transition":"120 pans across 2.82/2.84 LOD boundary",
        "sampledHoleFrames":transition_holes,
        "maxHolesInFrame":transition_max_holes,
        "recoveryMs":recovery_ms,
        "finalVisible":recovered.visible,
        "finalDetailed":recovered.detailed
    }));
    // The costly layers use different world samplers. Keep them in the same real asynchronous
    // render pipeline so a biome-only sweep cannot mask starvation on geology/forest rasters.
    let mut layer_records = Vec::new();
    for layer in [Layer::Temperature, Layer::Rainfall, Layer::Geology] {
        for bpp in [4., 8., 16.] {
            let (milliseconds, progress) = run.settle(final_center, bpp, layer);
            assert_eq!(
                progress.holes,
                0,
                "fallback holes on {} at {bpp}",
                layer.label()
            );
            assert_eq!(
                progress.detailed,
                progress.visible,
                "idle queue is not full coverage on {} at {bpp}: {progress:?}",
                layer.label()
            );
            eprintln!(
                "{} bpp={bpp:.3}: {milliseconds:.0} ms, {progress:?}",
                layer.label()
            );
            layer_records.push(
                serde_json::json!({"layer":layer.label(),"blocksPerPoint":bpp,
                "settleMs":milliseconds,"visible":progress.visible,"detailed":progress.detailed,
                "fallbackReady":progress.fallback_ready,"holes":progress.holes}),
            );
        }
    }
    // Drive the same stationary plan without pacing delay. This exposes plan/ack throughput
    // failures hidden by diagnostics that sleep between frames.
    let fast_center = Point {
        x: 250_000.,
        z: -175_000.,
    };
    let fast_bpp = 8.;
    let fast_layer = Layer::Rainfall;
    let fast_start = Instant::now();
    let mut frames = 0u64;
    let mut minimum_ready = usize::MAX;
    let mut last_ready = 0usize;
    let mut stalls = 0u32;
    let mut max_stall_frames = 0u32;
    while fast_start.elapsed() < Duration::from_secs(12) {
        run.frame(fast_center, fast_bpp, fast_layer);
        let progress = run.atlas.tile_stream_progress();
        minimum_ready = minimum_ready.min(progress.fallback_ready);
        if progress.fallback_ready == last_ready {
            stalls += 1;
            max_stall_frames = max_stall_frames.max(stalls);
        } else {
            last_ready = progress.fallback_ready;
            stalls = 0;
        }
        frames += 1;
    }
    let (fast_recovery_ms, fast_final) = run.settle(fast_center, fast_bpp, fast_layer);
    assert_eq!(fast_final.holes, 0);
    assert_eq!(fast_final.detailed, fast_final.visible);
    records.push(serde_json::json!({
        "stress":"12 seconds of unpaced identical view paints",
        "layer":fast_layer.label(),
        "blocksPerPoint":fast_bpp,
        "frames":frames,
        "minimumFallbackReady":minimum_ready,
        "maxConsecutiveFramesWithoutReadyIncrease":max_stall_frames,
        "recoveryMs":fast_recovery_ms,
        "visible":fast_final.visible,
        "detailed":fast_final.detailed
    }));
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "viewport":[size.x,size.y], "center":[center.x,center.z], "views":records,
        "layerViews":layer_records,
        "enginePid":run.atlas.engine_pid(),
        "scope":"real atlas render jobs and UI texture uploads; excludes native GPU presentation"
    })).unwrap());
}
