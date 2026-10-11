//! One bounded worker. Cancellation disconnects its queue before any join.
use crate::{Buffer, Event, QUERY_LIMIT, Query, SNAPSHOT_LIMIT};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};
/// A cancellable native scan. Polling and cancelling never wait for disk I/O.
pub struct Job {
    receiver: Option<Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Job {
    /// Compile first, then start a worker with a 32-message bounded queue.
    /// Hosts must wait for a cancelled worker to finish before replacing it.
    pub fn start(root: PathBuf, query: &Query, buffers: Vec<Buffer>) -> Result<Self, String> {
        if query.text.len() > QUERY_LIMIT
            || query.globs.iter().map(String::len).sum::<usize>() > QUERY_LIMIT
        {
            return Err("project query exceeds 16 KiB".into());
        }
        if buffers
            .iter()
            .try_fold(0usize, |sum, b| sum.checked_add(b.text.len()))
            .is_none_or(|sum| sum > SNAPSHOT_LIMIT)
        {
            return Err("open-buffer search snapshots exceed 32 MiB".into());
        }
        let regex = crate::compile(query)?;
        let mut overrides = ignore::overrides::OverrideBuilder::new(&root);
        for glob in &query.globs {
            overrides.add(glob).map_err(|e| e.to_string())?;
        }
        let overrides = overrides.build().map_err(|e| e.to_string())?;
        let (sender, receiver) = mpsc::sync_channel(32);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let worker = thread::Builder::new()
            .name("iridium-project-search".into())
            .spawn(move || {
                crate::walk::search(&root, &regex, &overrides, buffers, &sender, &flag);
            })
            .map_err(|e| format!("cannot start project search: {e}"))?;
        Ok(Self {
            receiver: Some(receiver),
            cancel,
            worker: Some(worker),
        })
    }
    /// Evaluate replacement previews off the host thread using the same bounded
    /// cancellation lifecycle as search. The result is one Preview event.
    pub fn preview(
        query: Query,
        hits: Vec<crate::Match>,
        replacement: String,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let worker = thread::Builder::new()
            .name("iridium-replacement-preview".into())
            .spawn(move || {
                let result = crate::preview_replacements(&query, &hits, &replacement, || {
                    flag.load(Ordering::Relaxed)
                });
                if sender.send(Event::Preview(result)).is_err() {
                    flag.store(true, Ordering::Relaxed);
                }
            })
            .map_err(|e| format!("cannot start replacement preview: {e}"))?;
        Ok(Self {
            receiver: Some(receiver),
            cancel,
            worker: Some(worker),
        })
    }
    /// Drain at most 64 events, allowing the UI to repaint between batches.
    pub fn poll(&mut self) -> Result<Vec<Event>, String> {
        let events: Vec<_> = self
            .receiver
            .as_ref()
            .map(|rx| rx.try_iter().take(64).collect())
            .unwrap_or_default();
        // Drain queued events before retiring the handle.
        if events.is_empty()
            && self.worker.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(worker) = self.worker.take()
        {
            worker
                .join()
                .map_err(|_| "project search worker panicked".to_owned())?;
        }
        Ok(events)
    }
    /// Disconnect queued results immediately; a blocked producer wakes as well.
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.receiver = None;
    }
    /// Whether the worker is still owned; call poll to reap a finished worker.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.worker.is_some()
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
