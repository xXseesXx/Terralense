//! Opt-in native event-loop responsiveness probe.
//!
//! The heartbeat runs independently of eframe and records the oldest repaint
//! request that has not yet been observed by `begin_frame`. This measures the
//! real native event loop, including graphics work, rather than egui CPU-only
//! submissions.
use crate::model::Point;
use eframe::egui;
use serde::Serialize;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

const HEARTBEAT: Duration = Duration::from_millis(16);
const RUN_FOR: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug)]
pub(super) struct Stress {
    pub center: Point,
    pub bpp: f64,
    /// True only during short search windows; callers can submit on the rising edge.
    pub search: bool,
    pub slice_y: Option<i32>,
}

#[derive(Serialize)]
struct Report {
    ui_pid: u32,
    engine_pid: Option<u32>,
    duration_ms: u64,
    frames: usize,
    workload: &'static str,
    wake_latency_ms: Summary,
    update_cpu_ms: Summary,
    native_frame_gap_ms: Summary,
}

#[derive(Serialize)]
struct Summary {
    p50: f64,
    p95: f64,
    max: f64,
}

pub(super) struct Monitor {
    surface_mode: bool,
    beneath_mode: bool,
    started: Instant,
    frame_started: Option<Instant>,
    last_frame: Option<Instant>,
    outstanding_wake: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    writer_done: Option<Receiver<()>>,
    writing: bool,
    report_path: PathBuf,
    engine_pid: Option<u32>,
    wake_latency_ms: Vec<f64>,
    update_cpu_ms: Vec<f64>,
    frame_gap_ms: Vec<f64>,
}

impl Monitor {
    pub(super) fn new(
        ctx: &egui::Context,
        report_path: PathBuf,
        surface_mode: bool,
        beneath_mode: bool,
    ) -> Self {
        let started = Instant::now();
        let heartbeat_ctx = ctx.clone();
        let outstanding_wake = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_wake = outstanding_wake.clone();
        let thread_stop = stop.clone();
        thread::Builder::new()
            .name("terralens-ui-heartbeat".into())
            .spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    thread::sleep(HEARTBEAT);
                    if thread_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let nanos = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                    let _ = thread_wake.compare_exchange(
                        0,
                        nanos.max(1),
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                    heartbeat_ctx.request_repaint();
                }
            })
            .expect("start TerraLens responsiveness heartbeat");
        Self {
            surface_mode,
            beneath_mode,
            started,
            frame_started: None,
            last_frame: None,
            outstanding_wake,
            stop,
            writer_done: None,
            writing: false,
            report_path,
            engine_pid: None,
            wake_latency_ms: Vec::with_capacity(1400),
            update_cpu_ms: Vec::with_capacity(1400),
            frame_gap_ms: Vec::with_capacity(1400),
        }
    }

    /// Call at the first line of each native `App::update`.
    pub(super) fn begin_frame(&mut self, engine_pid: Option<u32>) -> Option<Stress> {
        let now = Instant::now();
        self.engine_pid = engine_pid.or(self.engine_pid);
        if let Some(previous) = self.last_frame.replace(now) {
            self.frame_gap_ms
                .push(previous.elapsed().as_secs_f64() * 1000.);
        }
        let wake = self.outstanding_wake.swap(0, Ordering::AcqRel);
        if wake != 0 {
            let now_ns = self.started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            self.wake_latency_ms
                .push(now_ns.saturating_sub(wake) as f64 / 1_000_000.);
        }
        self.frame_started = Some(now);

        let elapsed = self.started.elapsed();
        if elapsed >= RUN_FOR {
            return None;
        }
        let t = elapsed.as_secs_f64();
        if self.beneath_mode {
            let phase = (t / 4.) as usize;
            return Some(Stress {
                center: Point {
                    x: -1184. + (t * 0.8).sin() * 90.,
                    z: -2248. + (t * 0.5).cos() * 90.,
                },
                bpp: if phase % 2 == 0 { 0.25 } else { 4. },
                search: (2.0..3.0).contains(&(t % 5.0)),
                slice_y: Some([96, 20, 195][phase % 3]),
            });
        }
        if self.surface_mode {
            return Some(Stress {
                center: Point {
                    x: -2244. + t * 9.,
                    z: -2316. + t * 4.,
                },
                bpp: if t < 10. { 0.125 } else { 0.25 },
                search: (5.0..6.0).contains(&(t % 10.0)),
                slice_y: None,
            });
        }
        // Walk continuously into unexplored terrain and alternate zoom levels so
        // the diagnostic exercises cold generation and texture replacement.
        let phase = (t / 3.0).floor() as i64;
        let center = Point {
            x: 180_000. + t * 7_500. + (phase.rem_euclid(2) as f64) * 120_000.,
            z: -240_000. - t * 4_200. - (phase.rem_euclid(3) as f64) * 80_000.,
        };
        let bpp = match ((t / 2.5).floor() as u64) % 4 {
            0 => 24.,
            1 => 48.,
            2 => 96.,
            _ => 32.,
        };
        let search = (5.0..6.0).contains(&(t % 10.0));
        Some(Stress {
            center,
            bpp,
            search,
            slice_y: None,
        })
    }

    /// Call after the rest of `App::update`. Returns true after the JSON report
    /// has been written off-thread and can safely be used to close the window.
    pub(super) fn end_frame(&mut self, _ctx: &egui::Context) -> bool {
        if let Some(start) = self.frame_started.take() {
            self.update_cpu_ms
                .push(start.elapsed().as_secs_f64() * 1000.);
        }
        if let Some(done) = &self.writer_done {
            return done.try_recv().is_ok();
        }
        if !self.writing && self.started.elapsed() >= RUN_FOR {
            self.writing = true;
            let (tx, rx) = mpsc::channel();
            self.writer_done = Some(rx);
            let report = self.report();
            let path = self.report_path.clone();
            thread::Builder::new()
                .name("terralens-responsiveness-report".into())
                .spawn(move || {
                    if let Some(parent) = path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if let Ok(json) = serde_json::to_vec_pretty(&report) {
                        if fs::write(path, json).is_ok() {
                            let _ = tx.send(());
                        }
                    }
                })
                .expect("start responsiveness report writer");
        }
        false
    }

    fn report(&self) -> Report {
        Report {
            ui_pid: std::process::id(),
            engine_pid: self.engine_pid,
            duration_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            frames: self.update_cpu_ms.len(),
            workload: if self.beneath_mode {
                "20s Beneath saved-floor and seed-biome pan/zoom, vertical slices and ore searches"
            } else if self.surface_mode {
                "20s seed surface generation, block-detail pan/zoom and periodic ore searches"
            } else {
                "20s cold-terrain pan/zoom with periodic searches"
            },
            wake_latency_ms: summarize(&self.wake_latency_ms),
            update_cpu_ms: summarize(&self.update_cpu_ms),
            native_frame_gap_ms: summarize(&self.frame_gap_ms),
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Do not join here: Drop may run on the native UI thread.
    }
}

fn summarize(samples: &[f64]) -> Summary {
    if samples.is_empty() {
        return Summary {
            p50: 0.,
            p95: 0.,
            max: 0.,
        };
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
    Summary {
        p50: percentile(0.50),
        p95: percentile(0.95),
        max: *sorted.last().unwrap_or(&0.),
    }
}
