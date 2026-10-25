//! Progress and cancellation for ffmpeg runs.
//!
//! The daemon runs each job on its own thread and installs a [`Control`] for
//! that thread with [`scoped`]. Every ffmpeg run started on the thread then
//! reports how far its output has got and is killed when the job is
//! cancelled, without the render, analyze and measure functions taking an
//! extra parameter. Runs on threads with no control behave as before.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// What a job's thread hands to its ffmpeg runs.
pub struct Control {
    cancel: Arc<AtomicBool>,
    on_progress: Box<dyn Fn(f64) + Send + Sync>,
}

impl Control {
    /// `on_progress` gets the seconds of output written so far, a few times a
    /// second, per ffmpeg run (a job with several runs sees it restart).
    pub fn new(cancel: Arc<AtomicBool>, on_progress: impl Fn(f64) + Send + Sync + 'static) -> Self {
        Self { cancel, on_progress: Box::new(on_progress) }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub(crate) fn progress(&self, seconds: f64) {
        (self.on_progress)(seconds);
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<Control>>> = const { RefCell::new(None) };
}

/// Run `f` with `control` installed for this thread.
pub fn scoped<R>(control: Arc<Control>, f: impl FnOnce() -> R) -> R {
    struct Reset(Option<Arc<Control>>);
    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let previous = CURRENT.with(|c| c.borrow_mut().replace(control));
    let _reset = Reset(previous);
    f()
}

/// The control installed for this thread, if any.
pub(crate) fn current() -> Option<Arc<Control>> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Report progress for this thread's job from work that is not an ffmpeg
/// run (a plugin's `$/progress`), in the same unit as the job's length.
pub fn report_progress(seconds: f64) {
    if let Some(c) = current() {
        c.progress(seconds);
    }
}

/// Whether this thread's job was cancelled (for work between ffmpeg runs).
pub fn cancelled() -> bool {
    current().is_some_and(|c| c.cancelled())
}
