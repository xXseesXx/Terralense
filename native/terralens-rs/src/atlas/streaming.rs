//! A replaceable, coalesced tile queue. Panning changes priority, not tile identity.
use super::{Spec, TileKey};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::{
    collections::{HashSet, VecDeque},
    sync::Mutex,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct TileTask {
    pub spec: Spec,
    pub key: TileKey,
    pub revision: u64,
}
#[derive(Default)]
struct Queue {
    epoch: u64,
    wanted: HashSet<TileKey>,
    queued: VecDeque<TileTask>,
    // Completed work stays active until the UI receives it, preventing duplicate jobs.
    active: HashSet<(u64, TileKey)>,
}
pub(super) struct TileScheduler {
    state: Mutex<Queue>,
    wake: Sender<()>,
    max_detail: usize,
}
pub(super) fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .saturating_sub(2)
        .clamp(1, 4)
}
impl TileScheduler {
    pub fn new() -> (Self, Receiver<()>) {
        let (wake, rx) = bounded(6);
        (
            Self {
                state: Mutex::new(Queue::default()),
                wake,
                max_detail: worker_count().saturating_sub(1).max(1),
            },
            rx,
        )
    }
    pub fn publish(&self, epoch: u64, wanted: HashSet<TileKey>, tasks: Vec<TileTask>) {
        let mut state = self.state.lock().unwrap();
        state.epoch = epoch;
        state.wanted = wanted;
        // Replace the old priority order. Offscreen jobs never sit ahead of a new view.
        state.queued = tasks
            .into_iter()
            .filter(|t| !state.active.contains(&(epoch, t.key)))
            .take(1024)
            .collect();
        let signals = state.queued.len().min(6);
        drop(state);
        for _ in 0..signals {
            let _ = self.wake.try_send(());
        }
    }
    pub fn take(&self) -> Option<TileTask> {
        let mut state = self.state.lock().unwrap();
        // Keep capacity for uncovered visible areas while center detail refines.
        // The wanted fine key distinguishes a visible preview from border prefetch.
        let detail_active = state
            .active
            .iter()
            .filter(|(epoch, key)| *epoch == state.epoch && key.detail)
            .count();
        let urgent = if detail_active >= self.max_detail {
            state.queued.iter().position(|task| {
                !task.key.detail
                    && state.wanted.contains(&TileKey {
                        detail: true,
                        ..task.key
                    })
            })
        } else {
            None
        };
        let task = match urgent {
            Some(index) => state.queued.remove(index)?,
            None => state.queued.pop_front()?,
        };
        state.active.insert((task.spec.epoch, task.key));
        Some(task)
    }
    pub fn wanted(&self, epoch: u64, key: TileKey) -> bool {
        let state = self.state.lock().unwrap();
        state.epoch == epoch && state.wanted.contains(&key)
    }
    pub fn complete(&self, epoch: u64, key: TileKey) {
        self.state.lock().unwrap().active.remove(&(epoch, key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{atlas::Layer, model::Settings};
    fn task(x: i32) -> TileTask {
        TileTask {
            spec: Spec {
                seed: 1,
                settings: Settings::default(),
                epoch: 1,
                display: crate::display::DisplayPrefs::default(),
                dimension: crate::dimension::Dimension::Overworld,
                slice_y: 96,
            },
            key: TileKey {
                x,
                z: 0,
                zoom: 0,
                layer: Layer::Biomes,
                detail: false,
            },
            revision: 1,
        }
    }
    #[test]
    fn uncovered_visible_tiles_keep_worker_capacity_while_details_are_active() {
        let (queue, _) = TileScheduler::new();
        let limit = queue.max_detail;
        let details: Vec<_> = (0..limit + 1)
            .map(|i| {
                let mut t = task(i as i32);
                t.key.detail = true;
                t
            })
            .collect();
        let coarse = task(100);
        let mut wanted: HashSet<_> = details.iter().map(|t| t.key).collect();
        wanted.extend([
            coarse.key,
            TileKey {
                detail: true,
                ..coarse.key
            },
        ]);
        let mut tasks = details.clone();
        tasks.push(coarse.clone());
        queue.publish(1, wanted, tasks);
        for _ in 0..limit {
            assert!(queue.take().unwrap().key.detail);
        }
        assert_eq!(
            queue.take().unwrap().key,
            coarse.key,
            "new visible coverage must not wait behind all detail jobs"
        );
    }

    #[test]
    fn pan_retains_overlapping_work_and_drops_obsolete_backlog() {
        let (queue, _) = TileScheduler::new();
        let a = task(0);
        let b = task(1);
        let c = task(2);
        queue.publish(1, [a.key, b.key].into(), vec![a.clone(), b.clone()]);
        assert_eq!(queue.take().unwrap().key, a.key);
        queue.publish(1, [a.key, c.key].into(), vec![c.clone(), a.clone()]);
        assert!(queue.wanted(1, a.key));
        assert!(!queue.wanted(1, b.key));
        assert_eq!(queue.take().unwrap().key, c.key);
        assert!(
            queue.take().is_none(),
            "overlapping in-flight tile must not restart"
        );
    }
    #[test]
    fn publishing_reorders_queued_work_and_completion_waits_for_ui() {
        let (queue, _) = TileScheduler::new();
        let a = task(0);
        let b = task(1);
        queue.publish(1, [a.key, b.key].into(), vec![a.clone(), b.clone()]);
        queue.publish(1, [a.key, b.key].into(), vec![b.clone(), a.clone()]);
        assert_eq!(queue.take().unwrap().key, b.key);
        queue.publish(1, [b.key].into(), vec![b.clone()]);
        assert!(queue.take().is_none());
        queue.complete(1, b.key);
        queue.publish(1, [b.key].into(), vec![b.clone()]);
        assert_eq!(queue.take().unwrap().key, b.key);
    }
    #[test]
    fn world_switch_invalidates_active_work_without_aliasing_new_jobs() {
        let (queue, _) = TileScheduler::new();
        let a = task(0);
        queue.publish(1, [a.key].into(), vec![a.clone()]);
        queue.take().unwrap();
        let mut b = a.clone();
        b.spec.epoch = 2;
        queue.publish(2, [b.key].into(), vec![b.clone()]);
        assert!(!queue.wanted(1, a.key));
        assert_eq!(queue.take().unwrap().spec.epoch, 2);
        queue.complete(1, a.key);
        assert!(queue.wanted(2, b.key));
    }
}
