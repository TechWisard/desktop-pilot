use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

/// A verified catalog scan has its own SQLite connection and COM apartment.
/// Dropping the monitor cancels any late commit, even when Windows cannot
/// interrupt a synchronous read from a disconnected shortcut directory.
pub(super) struct ScanWorker {
    pub receiver: mpsc::Receiver<Result<()>>,
    pub requests: Vec<i64>,
    pub follow_up_requests: Vec<i64>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for ScanWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

pub(super) fn start(home: &Path) -> ScanWorker {
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let home = home.to_path_buf();
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            let _com = native::Com::new()?;
            let store = Store::open(&home)?;
            refresh_apps_guarded(&store, || worker_cancelled.load(Ordering::SeqCst))
        })();
        let _ = sender.send(result);
    });
    ScanWorker {
        receiver,
        requests: Vec::new(),
        follow_up_requests: Vec::new(),
        cancelled,
    }
}
