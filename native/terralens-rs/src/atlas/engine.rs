//! Child-process boundary for atlas generation and save scanning.
//!
//! The GUI only enqueues small commands and receives finished replies. Serialization, pipe I/O,
//! world generation, NBT reads, and tile byte packing all happen away from the event thread.
use super::streaming::{TileScheduler, TileTask};
use super::{Job, Reply, TileKey};
#[cfg(test)]
use crossbeam_channel::unbounded;
use crossbeam_channel::{Receiver, Sender, bounded};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{self, BufReader, BufWriter, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_META: usize = 16 * 1024 * 1024;
const MAX_BULK: usize = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
enum CommandMessage {
    Reset {
        epoch: u64,
        search_revision: u64,
    },
    SearchRevision {
        serial: u64,
    },
    TaskRevision {
        serial: u64,
    },
    Plan {
        epoch: u64,
        wanted: HashSet<TileKey>,
        tasks: Vec<TileTask>,
    },
    Complete {
        epoch: u64,
        key: TileKey,
    },
    High(Job),
    Low(Job),
    Scan {
        epoch: u64,
        path: PathBuf,
    },
    Discover {
        path: PathBuf,
    },
    OpenSource {
        path: PathBuf,
        serial: u64,
    },
    Shutdown,
}

#[derive(Default)]
struct LatestPlan(Option<(u64, HashSet<TileKey>, Vec<TileTask>)>);

enum Control {
    Reset(u64, u64),
    SearchRevision(u64),
    TaskRevision(u64),
    Complete(u64, TileKey),
    Scan(u64, PathBuf),
    Discover(PathBuf),
    OpenSource(PathBuf, u64),
    Shutdown,
}

/// Nonblocking facade used by Atlas. `replies` is bounded to provide natural backpressure all
/// the way to the engine workers; it is drained on the UI thread a few items at a time.
pub(super) struct Client {
    pub replies: Receiver<Reply>,
    high: Sender<Job>,
    low: Sender<Job>,
    control: Sender<Control>,
    plan: Arc<Mutex<LatestPlan>>,
    plan_wake: Sender<()>,
    pid: Option<u32>,
    stopping: Arc<AtomicBool>,
    #[cfg(test)]
    pub(super) test_out: Sender<Reply>,
}

impl Client {
    pub(super) fn new(ctx: &egui::Context) -> Self {
        #[cfg(not(test))]
        let (reply_tx, replies): (Sender<Reply>, Receiver<Reply>) = bounded(64);
        #[cfg(test)]
        let (reply_tx, replies) = unbounded();
        let (high_tx, high_rx) = bounded(48);
        let (low_tx, low_rx) = bounded(8);
        let (control_tx, control_rx) = crossbeam_channel::unbounded();
        let (plan_wake, plan_rx) = bounded(1);
        let plan = Arc::new(Mutex::new(LatestPlan::default()));
        let stopping = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        if cfg!(test) {
            return Self {
                replies,
                high: high_tx,
                low: low_tx,
                control: control_tx,
                plan,
                plan_wake,
                pid: None,
                stopping,
                test_out: reply_tx,
            };
        }
        let exe = std::env::current_exe();
        let child_result = exe.map_err(|e| e.to_string()).and_then(|exe| {
            // `nice` reserves CPU time for the window manager and interactive UI. It execs the
            // Rust binary, so the child remains the same process and PID.
            let mut cmd = Command::new("nice");
            cmd.args(["-n", "5"]).arg(exe).arg("--engine");
            cmd.env("RAYON_NUM_THREADS", "2");
            cmd.stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            cmd.spawn()
                .or_else(|_| {
                    let mut fallback = Command::new(std::env::current_exe()?);
                    fallback
                        .arg("--engine")
                        .env("RAYON_NUM_THREADS", "2")
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                })
                .map_err(|e| e.to_string())
        });
        let mut pid = None;
        match child_result {
            Ok(mut child) => {
                pid = Some(child.id());
                let stdin = child.stdin.take();
                let stdout = child.stdout.take();
                let (control, high, low, plan, plan_rx) =
                    (control_rx, high_rx, low_rx, plan.clone(), plan_rx);
                let writer_stdin = stdin;
                thread::Builder::new()
                    .name("atlas-ipc-writer".into())
                    .spawn(move || {
                        if let Some(stdin) = writer_stdin {
                            writer_loop(stdin, control, high, low, plan, plan_rx);
                        }
                    })
                    .ok();
                if let Some(stdout) = stdout {
                    let tx = reply_tx.clone();
                    let repaint = ctx.clone();
                    thread::Builder::new()
                        .name("atlas-ipc-reader".into())
                        .spawn(move || {
                            let mut input = BufReader::new(stdout);
                            loop {
                                match read_reply(&mut input) {
                                    Ok(Some(reply)) => {
                                        if tx.send(reply).is_err() {
                                            break;
                                        }
                                        repaint.request_repaint();
                                    }
                                    Ok(None) => {
                                        let _ = tx
                                            .send(Reply::EngineError("Atlas engine exited".into()));
                                        repaint.request_repaint();
                                        break;
                                    }
                                    Err(error) => {
                                        let _ = tx.send(Reply::EngineError(error.to_string()));
                                        repaint.request_repaint();
                                        break;
                                    }
                                }
                            }
                        })
                        .ok();
                }
                // The supervisor never waits on the UI. A stuck child is forcibly
                // reaped after the short graceful shutdown deadline.
                let child_stopping = stopping.clone();
                thread::Builder::new()
                    .name("atlas-engine-reaper".into())
                    .spawn(move || {
                        let mut closing = None;
                        loop {
                            match child.try_wait() {
                                Ok(Some(_)) | Err(_) => break,
                                Ok(None) => {}
                            }
                            if child_stopping.load(Ordering::Relaxed) {
                                let since = closing.get_or_insert_with(Instant::now);
                                if since.elapsed() >= Duration::from_millis(500) {
                                    let _ = child.kill();
                                    let _ = child.wait();
                                    break;
                                }
                            }
                            thread::sleep(Duration::from_millis(20));
                        }
                    })
                    .ok();
            }
            Err(error) => {
                let _ = reply_tx.send(Reply::EngineError(format!(
                    "Could not start atlas engine: {error}"
                )));
            }
        }
        #[cfg(test)]
        let test_out = reply_tx.clone();
        Self {
            replies,
            high: high_tx,
            low: low_tx,
            control: control_tx,
            plan,
            plan_wake,
            pid,
            stopping,
            #[cfg(test)]
            test_out,
        }
    }

    pub(super) fn high(&self, job: Job) -> bool {
        self.high.try_send(job).is_ok()
    }
    pub(super) fn low(&self, job: Job) -> bool {
        self.low.try_send(job).is_ok()
    }
    pub(super) fn reset(&self, epoch: u64, search_revision: u64) {
        let _ = self
            .control
            .try_send(Control::Reset(epoch, search_revision));
    }
    pub(super) fn search_revision(&self, serial: u64) {
        let _ = self.control.try_send(Control::SearchRevision(serial));
    }
    pub(super) fn task_revision(&self, serial: u64) {
        let _ = self.control.try_send(Control::TaskRevision(serial));
    }
    pub(super) fn plan(&self, epoch: u64, wanted: HashSet<TileKey>, tasks: Vec<TileTask>) {
        self.plan.lock().unwrap_or_else(|e| e.into_inner()).0 = Some((epoch, wanted, tasks));
        let _ = self.plan_wake.try_send(());
    }
    pub(super) fn complete(&self, epoch: u64, key: TileKey) {
        let _ = self.control.try_send(Control::Complete(epoch, key));
    }
    pub(super) fn scan(&self, epoch: u64, path: PathBuf) {
        let _ = self.control.try_send(Control::Scan(epoch, path));
    }
    pub(super) fn discover(&self, path: PathBuf) {
        let _ = self.control.try_send(Control::Discover(path));
    }
    pub(super) fn open_source(&self, path: PathBuf, serial: u64) {
        let _ = self.control.try_send(Control::OpenSource(path, serial));
    }
    pub(super) fn pid(&self) -> Option<u32> {
        self.pid
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        let _ = self.control.try_send(Control::Shutdown);
    }
}

fn writer_loop(
    stdin: impl Write,
    control: Receiver<Control>,
    high: Receiver<Job>,
    low: Receiver<Job>,
    plan: Arc<Mutex<LatestPlan>>,
    plan_wake: Receiver<()>,
) {
    let mut out = BufWriter::new(stdin);
    loop {
        let message = if let Ok(c) = control.try_recv() {
            Some(match c {
                Control::Reset(epoch, search_revision) => CommandMessage::Reset {
                    epoch,
                    search_revision,
                },
                Control::SearchRevision(serial) => CommandMessage::SearchRevision { serial },
                Control::TaskRevision(serial) => CommandMessage::TaskRevision { serial },
                Control::Complete(epoch, key) => CommandMessage::Complete { epoch, key },
                Control::Scan(epoch, path) => CommandMessage::Scan { epoch, path },
                Control::Discover(path) => CommandMessage::Discover { path },
                Control::OpenSource(path, serial) => CommandMessage::OpenSource { path, serial },
                Control::Shutdown => CommandMessage::Shutdown,
            })
        } else if let Ok(job) = high.try_recv() {
            Some(CommandMessage::High(job))
        } else if let Some((epoch, wanted, tasks)) = take_plan(&plan) {
            Some(CommandMessage::Plan {
                epoch,
                wanted,
                tasks,
            })
        } else if let Ok(job) = low.try_recv() {
            Some(CommandMessage::Low(job))
        } else {
            crossbeam_channel::select_biased! {
                recv(control) -> c => match c {
                    Ok(Control::Reset(epoch, search_revision)) => Some(CommandMessage::Reset { epoch, search_revision }),
                    Ok(Control::SearchRevision(serial)) => Some(CommandMessage::SearchRevision { serial }),
                    Ok(Control::TaskRevision(serial)) => Some(CommandMessage::TaskRevision { serial }),
                    Ok(Control::Complete(epoch, key)) => Some(CommandMessage::Complete { epoch, key }),
                    Ok(Control::Scan(epoch, path)) => Some(CommandMessage::Scan { epoch, path }),
                    Ok(Control::Discover(path)) => Some(CommandMessage::Discover { path }),
                    Ok(Control::OpenSource(path, serial)) => Some(CommandMessage::OpenSource { path, serial }),
                    Ok(Control::Shutdown) => Some(CommandMessage::Shutdown), Err(_) => return,
                },
                recv(high) -> j => j.ok().map(CommandMessage::High),
                recv(plan_wake) -> _ => continue,
                recv(low) -> j => j.ok().map(CommandMessage::Low),
                default(Duration::from_millis(20)) => continue,
            }
        };
        let Some(message) = message else { continue };
        let shutdown = matches!(message, CommandMessage::Shutdown);
        if write_frame(&mut out, &message, &[]).is_err() {
            return;
        }
        if shutdown {
            return;
        }
    }
}

// Release this mutex before selecting/sleeping or writing to the pipe. An if-let
// scrutinee guard can otherwise live through the entire else/select expression.
fn take_plan(plan: &Mutex<LatestPlan>) -> Option<(u64, HashSet<TileKey>, Vec<TileTask>)> {
    plan.lock().unwrap_or_else(|e| e.into_inner()).0.take()
}

fn job_epoch(job: &Job) -> u64 {
    match job {
        Job::Tile(spec, ..)
        | Job::Probe(spec, ..)
        | Job::Trees(spec, ..)
        | Job::Features(spec, ..)
        | Job::Surface(spec, ..)
        | Job::Search(spec, ..)
        | Job::Analyze(spec, ..)
        | Job::Locate(spec, ..)
        | Job::Export(spec, ..) => spec.epoch,
    }
}

// One queued job per kind is sufficient: the UI only accepts the newest serial
// for that kind. Never block the command reader behind generation or output
// backpressure: it must always receive cancellation, acknowledgements and EOF.
fn enqueue_latest(tx: &Sender<Job>, rx: &Receiver<Job>, job: Job, epoch: u64) {
    if job_epoch(&job) != epoch {
        return;
    }
    let kind = std::mem::discriminant(&job);
    let mut waiting: Vec<_> = rx
        .try_iter()
        .filter(|old| job_epoch(old) == epoch && std::mem::discriminant(old) != kind)
        .collect();
    waiting.push(job);
    for next in waiting {
        // The command reader is the only producer; at most eight job kinds fit.
        let _ = tx.try_send(next);
    }
}

/// Child entrypoint. It is called by main before any display backend is initialized.
pub(super) fn run() -> Result<(), String> {
    let (high_tx, high_rx) = bounded(48);
    let (low_tx, low_rx) = bounded(8);
    let (out, out_rx) = bounded(24);
    let (scan_tx, scan_rx) = bounded::<(u64, PathBuf)>(1);
    let (source_tx, source_rx) = bounded::<(u64, PathBuf)>(1);
    let (tile_work, tile_wake) = TileScheduler::new();
    let tile_work = Arc::new(tile_work);
    let epoch = Arc::new(AtomicU64::new(1));
    let search_revision = Arc::new(AtomicU64::new(0));
    let task_revision = Arc::new(AtomicU64::new(0));
    let scan_out = out.clone();
    let scan_queue = scan_rx.clone();
    thread::Builder::new()
        .name("atlas-save-scan".into())
        .spawn(move || {
            while let Ok((e, path)) = scan_queue.recv() {
                let result = crate::prism::scan_structures(&path);
                // Save scans belong to a storage revision, independently of raster/Y changes.
                // Atlas rejects a reply after switching save or dimension, but retains it while
                // the user scrubs height or changes display colors.
                if scan_out.send(Reply::Structures(e, result)).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| format!("create save scanner: {e}"))?;
    let source_out = out.clone();
    let source_queue = source_rx.clone();
    thread::Builder::new()
        .name("atlas-source-import".into())
        .spawn(move || {
            while let Ok((serial, path)) = source_queue.recv() {
                let result = crate::world_files::resolve_source(&path);
                if source_out
                    .send(Reply::OpenedSource(serial, result))
                    .is_err()
                {
                    break;
                }
            }
        })
        .map_err(|e| format!("create source importer: {e}"))?;
    let count = super::streaming::worker_count();
    for i in 0..count {
        let (hi, lo, tx, ep, tw, wake, sr, tr) = (
            high_rx.clone(),
            low_rx.clone(),
            out.clone(),
            epoch.clone(),
            tile_work.clone(),
            tile_wake.clone(),
            search_revision.clone(),
            task_revision.clone(),
        );
        thread::Builder::new()
            .name(format!("atlas-engine-{i}"))
            .spawn(move || {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    super::worker(hi, lo, tx, ep, tw, wake, sr, tr);
                }))
                .is_err()
                {
                    // A dead worker must not leave its active tiles pending forever.
                    std::process::exit(70);
                }
            })
            .map_err(|e| format!("create atlas worker: {e}"))?;
    }
    let _output = thread::Builder::new()
        .name("atlas-engine-output".into())
        .spawn(move || {
            let stdout = io::stdout();
            let mut writer = BufWriter::new(stdout.lock());
            while let Ok(reply) = out_rx.recv() {
                if write_reply(&mut writer, &reply).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| format!("create output thread: {e}"))?;

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    loop {
        let message: Option<CommandMessage> = match read_frame(&mut reader) {
            Ok(v) => v,
            Err(e) => {
                epoch.fetch_add(1, Ordering::Relaxed);
                return Err(e.to_string());
            }
        };
        let Some(message) = message else { break };
        match message {
            CommandMessage::Reset {
                epoch: e,
                search_revision: sr,
            } => {
                epoch.store(e, Ordering::Relaxed);
                search_revision.store(sr, Ordering::Relaxed);
                tile_work.publish(e, HashSet::new(), Vec::new());
            }
            CommandMessage::SearchRevision { serial } => {
                search_revision.store(serial, Ordering::Relaxed)
            }
            CommandMessage::TaskRevision { serial } => {
                task_revision.store(serial, Ordering::Relaxed)
            }
            CommandMessage::Plan {
                epoch: e,
                wanted,
                tasks,
            } => {
                if e == epoch.load(Ordering::Relaxed) {
                    tile_work.publish(e, wanted, tasks);
                }
            }
            CommandMessage::Complete { epoch: e, key } => tile_work.complete(e, key),
            CommandMessage::High(job) => {
                enqueue_latest(&high_tx, &high_rx, job, epoch.load(Ordering::Relaxed))
            }
            CommandMessage::Low(job) => {
                enqueue_latest(&low_tx, &low_rx, job, epoch.load(Ordering::Relaxed))
            }
            CommandMessage::Scan { epoch: e, path } => {
                // A newly selected save replaces the waiting scan, never spawning
                // a new competing reader for every world switch.
                let _ = scan_rx.try_recv();
                let _ = scan_tx.try_send((e, path));
            }
            CommandMessage::Discover { path } => {
                let tx = out.clone();
                let _ = thread::Builder::new()
                    .name("atlas-world-discovery".into())
                    .spawn(move || {
                        let worlds = crate::prism::discover_worlds(&path);
                        let _ = tx.send(Reply::Worlds(worlds));
                    });
            }
            CommandMessage::OpenSource { path, serial } => {
                // Coalesce queued imports. The single source worker owns all filesystem
                // reads, while the command reader stays available for cancellation/EOF.
                let _ = source_rx.try_recv();
                let _ = source_tx.try_send((serial, path));
            }
            CommandMessage::Shutdown => break,
        }
    }
    epoch.fetch_add(1, Ordering::Relaxed);
    drop(high_tx);
    drop(low_tx);
    drop(out);
    Ok(())
}

fn write_frame(w: &mut impl Write, meta: &impl Serialize, bulk: &[u8]) -> io::Result<()> {
    let json = serde_json::to_vec(meta).map_err(io::Error::other)?;
    if json.len() > MAX_META || bulk.len() > MAX_BULK {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "atlas IPC frame exceeds limit",
        ));
    }
    w.write_all(&(json.len() as u32).to_le_bytes())?;
    w.write_all(&(bulk.len() as u32).to_le_bytes())?;
    w.write_all(&json)?;
    w.write_all(bulk)?;
    w.flush()
}
fn read_frame<T: for<'de> Deserialize<'de>>(r: &mut impl Read) -> io::Result<Option<T>> {
    let mut header = [0u8; 8];
    match r.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let m = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
    let b = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
    if m > MAX_META || b > MAX_BULK {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "atlas IPC frame exceeds limit",
        ));
    }
    let mut json = vec![0; m];
    let mut bulk = vec![0; b];
    r.read_exact(&mut json)?;
    r.read_exact(&mut bulk)?;
    if b != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected bulk payload on command",
        ));
    }
    serde_json::from_slice(&json)
        .map(Some)
        .map_err(io::Error::other)
}
fn write_reply(w: &mut impl Write, reply: &Reply) -> io::Result<()> {
    if let Reply::Tile(_, _, tile) = reply {
        let mut bulk = Vec::with_capacity(tile.pixels.len() * 4 + tile.biomes.len() * 2);
        for color in &tile.pixels {
            bulk.extend_from_slice(&color.to_array());
        }
        for biome in &tile.biomes {
            bulk.extend_from_slice(&biome.to_le_bytes());
        }
        write_frame(w, reply, &bulk)
    } else {
        write_frame(w, reply, &[])
    }
}
fn read_reply(r: &mut impl Read) -> io::Result<Option<Reply>> {
    let mut header = [0u8; 8];
    match r.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let m = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
    let b = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
    if m > MAX_META || b > MAX_BULK {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "atlas IPC frame exceeds limit",
        ));
    }
    let mut json = vec![0; m];
    let mut bulk = vec![0; b];
    r.read_exact(&mut json)?;
    r.read_exact(&mut bulk)?;
    let mut reply: Reply = serde_json::from_slice(&json).map_err(io::Error::other)?;
    if let Reply::Tile(_, _, tile) = &mut reply {
        let resolution = tile.resolution;
        if resolution == 0 || resolution > 256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid tile resolution",
            ));
        }
        let side = resolution
            .checked_add(2)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid tile resolution"))?;
        let pixel_bytes = side
            .checked_mul(side)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "tile dimensions overflow")
            })?;
        let biome_count = resolution.checked_mul(resolution).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "tile dimensions overflow")
        })?;
        let expected = pixel_bytes
            .checked_add(biome_count * 2)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "tile payload overflow"))?;
        if resolution > 256 || bulk.len() != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid tile payload size",
            ));
        }
        tile.pixels.extend(
            bulk[..pixel_bytes]
                .chunks_exact(4)
                .map(|p| egui::Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3])),
        );
        tile.biomes.extend(
            bulk[pixel_bytes..]
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]])),
        );
    } else if !bulk.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bulk data on non-tile reply",
        ));
    }
    Ok(Some(reply))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ingress_coalesces_superseded_jobs_without_blocking_cancellation() {
        use crate::model::{Point, Settings};
        let spec = super::super::Spec {
            seed: 1,
            settings: Settings::default(),
            epoch: 2,
            display: crate::display::DisplayPrefs::default(),
            dimension: crate::dimension::Dimension::Overworld,
            slice_y: 96,
        };
        let (tx, rx) = bounded(3);
        enqueue_latest(
            &tx,
            &rx,
            Job::Features(
                spec.clone(),
                super::super::FeatureTileKey {
                    x: 0,
                    z: 0,
                    tier: 0,
                    scale: 0,
                    ores: true,
                    structures: true,
                },
                9,
            ),
            2,
        );
        enqueue_latest(&tx, &rx, Job::Trees(spec.clone(), vec![], 1., 10), 2);
        for serial in 0..1000 {
            enqueue_latest(
                &tx,
                &rx,
                Job::Probe(spec.clone(), Point::default(), false, serial),
                2,
            );
        }
        assert_eq!(
            rx.len(),
            3,
            "one pending job per kind, retaining unrelated work"
        );
        let jobs: Vec<_> = rx.try_iter().collect();
        assert!(jobs.iter().any(|job| matches!(job, Job::Features(_, _, 9))));
        assert!(
            jobs.iter()
                .any(|job| matches!(job, Job::Trees(_, _, _, 10)))
        );
        assert!(
            jobs.iter()
                .any(|job| matches!(job, Job::Probe(_, _, false, 999)))
        );
        let mut old = spec.clone();
        old.epoch = 1;
        enqueue_latest(&tx, &rx, Job::Probe(old, Point::default(), false, 1000), 2);
        assert!(rx.is_empty(), "old world work must not enter the new queue");
    }

    #[test]
    fn framed_json_round_trips_and_rejects_oversized_lengths() {
        let msg = CommandMessage::Shutdown;
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &msg, &[]).unwrap();
        assert!(matches!(
            read_frame::<CommandMessage>(&mut bytes.as_slice()).unwrap(),
            Some(CommandMessage::Shutdown)
        ));
        let oversized = [(MAX_META as u32 + 1).to_le_bytes(), 0u32.to_le_bytes()].concat();
        let error = match read_frame::<CommandMessage>(&mut oversized.as_slice()) {
            Err(error) => error,
            Ok(_) => panic!("oversized frame was accepted"),
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let msg = CommandMessage::TaskRevision { serial: 42 };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &msg, &[]).unwrap();
        assert!(matches!(
            read_frame::<CommandMessage>(&mut bytes.as_slice()).unwrap(),
            Some(CommandMessage::TaskRevision { serial: 42 })
        ));
    }

    #[test]
    fn older_serialized_specs_default_to_natural_display() {
        let legacy = r#"{"seed":1,"settings":{"temperature_scale":20000.0,"temperature_constant":0.0,"rainfall_scale":20000.0,"rainfall_constant":0.0,"continentalness":0.5,"grass_density":1.0},"epoch":2}"#;
        let spec: super::super::Spec = serde_json::from_str(legacy).unwrap();
        assert_eq!(spec.display, crate::display::DisplayPrefs::default());
        assert_eq!(spec.dimension, crate::dimension::Dimension::Overworld);
        assert_eq!(spec.slice_y, 96);
    }

    #[test]
    fn tile_reply_keeps_packed_colors_and_biome_plane() {
        let tile = super::super::CpuTile {
            key: TileKey {
                zoom: 0,
                x: -1,
                z: 2,
                layer: super::super::Layer::Temperature,
                detail: false,
            },
            resolution: 1,
            pixels: vec![egui::Color32::from_rgba_unmultiplied(80, 120, 200, 128); 9],
            biomes: vec![493],
        };
        let expected_pixels = tile.pixels.clone();
        let mut bytes = Vec::new();
        write_reply(&mut bytes, &Reply::Tile(4, 7, tile)).unwrap();
        let Reply::Tile(epoch, revision, decoded) =
            read_reply(&mut bytes.as_slice()).unwrap().unwrap()
        else {
            panic!("expected tile reply")
        };
        assert_eq!((epoch, revision), (4, 7));
        assert_eq!(decoded.pixels, expected_pixels);
        assert_eq!(decoded.biomes, [493]);
    }
}
