//! Manual Wayland regression probe for eframe's hidden-window swap behavior.
//!
//! Run with `cargo run --release --example wayland_background_probe -- --vsync`
//! to reproduce eframe's default (blocking) swap interval, or omit `--vsync` to
//! use the recommended non-blocking interval. Cover the window with another
//! app or move it to another workspace, wait a few seconds, then reveal it.
//! The watchdog continues printing while the UI event loop is blocked.

use eframe::egui;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

struct Probe {
    frames: Arc<AtomicU64>,
    vsync: bool,
    cover: bool,
}

impl eframe::App for Probe {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let frame = self.frames.fetch_add(1, Ordering::Relaxed) + 1;
        if !self.cover {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Wayland background responsiveness probe");
                ui.label(format!("Swap interval waits for vsync: {}", self.vsync));
                ui.label(format!("UI frame: {frame}"));
                ui.label("Cover this window or move it to another workspace.");
                ui.label("The terminal watchdog reports UI frame progress once per second.");
            });
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.painter()
                    .rect_filled(ui.max_rect(), 0., egui::Color32::BLACK);
            });
        }
    }
}

fn main() -> eframe::Result {
    let vsync = std::env::args().skip(1).any(|arg| arg == "--vsync");
    let cover = std::env::args().skip(1).any(|arg| arg == "--cover");
    let mode = if cover {
        "Cover"
    } else if vsync {
        "Wait"
    } else {
        "DontWait"
    };
    let frames = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let reporter_frames = frames.clone();
    let reporter_stop = stop.clone();
    let started = Instant::now();
    let reporter = thread::Builder::new()
        .name("wayland-liveness-watchdog".into())
        .spawn(move || {
            let mut last = 0;
            while !reporter_stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(1));
                let current = reporter_frames.load(Ordering::Relaxed);
                println!(
                    "t={:.1}s ui_frames={} frames_last_second={}",
                    started.elapsed().as_secs_f64(),
                    current,
                    current.saturating_sub(last)
                );
                last = current;
            }
        })
        .expect("start watchdog");

    eprintln!(
        "WAYLAND_BACKGROUND_PROBE_READY mode={mode} pid={}",
        std::process::id()
    );

    let result = eframe::run_native(
        &format!("TerraLens Wayland Liveness Probe · {mode}"),
        eframe::NativeOptions {
            vsync,
            viewport: if cover {
                egui::ViewportBuilder::default().with_fullscreen(true)
            } else {
                egui::ViewportBuilder::default().with_inner_size([640., 240.])
            },
            ..Default::default()
        },
        Box::new(move |_| {
            Ok(Box::new(Probe {
                frames,
                vsync,
                cover,
            }))
        }),
    );
    stop.store(true, Ordering::Relaxed);
    let _ = reporter.join();
    result
}
