//! Reproducible CPU/UI streaming exercise using the real asynchronous viewer pipeline.
//! cargo run --release --locked --example streaming
use eframe::egui;
use std::time::{Duration, Instant};
use terralens::{
    atlas::{Atlas, Layer},
    model::{Point, Settings},
};

struct Runner {
    ctx: egui::Context,
    atlas: Atlas,
    start: Instant,
    times: Vec<f64>,
}
impl Runner {
    fn frame(&mut self, center: Point, bpp: f64) {
        let begin = Instant::now();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            time: Some(self.start.elapsed().as_secs_f64()),
            ..Default::default()
        };
        let _ = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.atlas.paint(
                    ui,
                    ui.available_rect_before_wrap(),
                    center,
                    bpp,
                    Layer::Biomes,
                );
            });
        });
        self.times.push(begin.elapsed().as_secs_f64() * 1000.);
    }
    fn settle(&mut self, center: Point, bpp: f64) -> f64 {
        let begin = Instant::now();
        loop {
            self.frame(center, bpp);
            if !self.atlas.is_loading() {
                break;
            }
            assert!(
                begin.elapsed() < Duration::from_secs(120),
                "stream did not finish: {}",
                self.atlas.status()
            );
            std::thread::sleep(Duration::from_millis(8));
        }
        begin.elapsed().as_secs_f64() * 1000.
    }
}
fn main() {
    if std::env::args().any(|a| a == "--engine") {
        terralens::atlas::run_engine().unwrap();
        return;
    }
    let ctx = egui::Context::default();
    let mut atlas = Atlas::new(&ctx, 7787531943268589495, Settings::default());
    atlas.set_marker_options(false, false, "");
    let mut run = Runner {
        ctx,
        atlas,
        start: Instant::now(),
        times: Vec::new(),
    };
    let origin = Point {
        x: -2244.,
        z: -2316.,
    };
    let cold = run.settle(origin, 32.);
    eprintln!("Initial view refined in {cold:.0} ms");
    run.times.clear();
    for step in 0..90 {
        run.frame(
            Point {
                x: origin.x + step as f64 * 128.,
                z: origin.z + step as f64 * 64.,
            },
            32.,
        );
        std::thread::sleep(Duration::from_millis(8));
    }
    let pan_settle = run.settle(
        Point {
            x: origin.x + 89. * 128.,
            z: origin.z + 89. * 64.,
        },
        32.,
    );
    eprintln!("Moving view refined after {pan_settle:.0} ms settling");
    for (stage, bpp) in [40., 56., 80., 128., 256., 512.].into_iter().enumerate() {
        for step in 0..15 {
            let offset = (stage * 15 + step) as f64;
            run.frame(
                Point {
                    x: 20_000. + offset * 1024.,
                    z: -20_000. - offset * 768.,
                },
                bpp,
            );
            std::thread::sleep(Duration::from_millis(8));
        }
    }
    let far = run.settle(
        Point {
            x: 111_136.,
            z: -88_352.,
        },
        512.,
    );
    eprintln!("Distant zoomed-out view refined after {far:.0} ms settling");
    let revisit = run.settle(origin, 32.);
    run.times.sort_by(f64::total_cmp);
    let percentile = |p: f64| run.times[((run.times.len() - 1) as f64 * p) as usize];
    let rss = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse::<u64>().ok())
        })
        .unwrap_or(0);
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "viewport":[1440,900],"coldRefinedMs":cold,"panSettleMs":pan_settle,
        "farZoomSettleMs":far,"revisitRefinedMs":revisit,"uiFrameP50Ms":percentile(0.5),
        "uiFrameP95Ms":percentile(0.95),"uiFrameMaxMs":percentile(1.),"rssKiB":rss,
        "frames":run.times.len(),"enginePid":run.atlas.engine_pid(),"scope":"isolated Rust generation process and egui UI submission; excludes native GPU presentation"
    })).unwrap());
}
