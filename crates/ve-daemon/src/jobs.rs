//! Background jobs: long calls (export, previews, analysis, measurements)
//! that answer `{job_id}` at once when called with `background: true`.
//!
//! Each job runs on its own thread with a [`ve_render::control::Control`]
//! installed, so its ffmpeg runs report progress and stop on cancel. At most
//! [`MAX_RUNNING`] run at once; the rest wait their turn. Jobs live in the
//! daemon's memory: stopping the daemon cancels the ones still running, and
//! only the last [`KEEP_FINISHED`] finished jobs are kept.
//!
//! Every subscriber (`events.subscribe`) gets `job.progress` while a job runs
//! and one `job.finished` when it ends. Clients that do not subscribe poll
//! `jobs.get`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ve_protocol::{ErrorCode, RpcError};
use ve_render::control::{self, Control};

use crate::Snapshot;
use crate::events::Events;

/// Jobs that run at the same time; more wait in the queue.
pub const MAX_RUNNING: usize = 2;
/// Finished jobs kept for `jobs.get` / `jobs.list`.
pub const KEEP_FINISHED: usize = 50;
/// Least time between two `job.progress` notifications of one job.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// Params of methods that can run as a job.
pub trait Background {
    /// The caller asked for a background job.
    fn background(&self) -> bool;
    /// Seconds of media the job works through, so ffmpeg's position becomes a fraction.
    fn expected_secs(&self, _snapshot: &Snapshot) -> Option<f64> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn finished(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

/// One job as `jobs.get` shows it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobInfo {
    pub job_id: String,
    /// The method running (`render.export`, `analyze.silence`, ...).
    pub method: String,
    pub state: JobState,
    /// 0 to 1 while running, when the job knows how long its media is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// When the job was started, finished: milliseconds since the Unix epoch.
    pub created_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_ms: Option<u64>,
    /// The method's normal result, once done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The JSON-RPC error (`code`, `message`, `data`), once failed or cancelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// What a method called with `background: true` answers.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct JobStarted {
    pub job_id: String,
    pub state: JobState,
}

/// A method's result, or the job running it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum MaybeJob<R> {
    Done(R),
    Started(JobStarted),
}

struct Job {
    info: JobInfo,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    jobs: VecDeque<Job>,
    running: usize,
    /// When the last job finished, so the daemon's idle timer counts from then.
    last_finished: Option<Instant>,
}

/// The session's jobs.
pub struct Jobs {
    inner: Arc<(Mutex<Inner>, Condvar)>,
    events: Events,
}

impl Jobs {
    pub fn new(events: Events) -> Self {
        Self { inner: Arc::default(), events }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queue `work` as a job for `method`. `expected_secs` turns ffmpeg's
    /// position into a fraction.
    pub fn start(
        &self,
        method: &str,
        expected_secs: Option<f64>,
        work: impl FnOnce() -> Result<Value, RpcError> + Send + 'static,
    ) -> JobStarted {
        let job_id = ve_model::Id::new("job").to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let info = JobInfo {
            job_id: job_id.clone(),
            method: method.to_owned(),
            state: JobState::Queued,
            progress: None,
            created_ms: now_ms(),
            finished_ms: None,
            result: None,
            error: None,
        };
        {
            let mut inner = self.lock();
            inner.jobs.push_back(Job { info, cancel: cancel.clone() });
            prune(&mut inner);
        }
        let inner = self.inner.clone();
        let events = self.events.clone();
        let id = job_id.clone();
        std::thread::Builder::new()
            .name(format!("ve-{job_id}"))
            .spawn(move || run(&inner, &events, &id, cancel, expected_secs, work))
            .expect("spawning a job thread");
        JobStarted { job_id, state: JobState::Queued }
    }

    pub fn get(&self, job_id: &str) -> Result<JobInfo, RpcError> {
        self.lock()
            .jobs
            .iter()
            .find(|j| j.info.job_id == job_id)
            .map(|j| j.info.clone())
            .ok_or_else(|| RpcError::invalid_params(format!("no job `{job_id}` (finished jobs are kept until {KEEP_FINISHED} newer ones finish)")))
    }

    /// Every job, oldest first, without results (`jobs.get` has them).
    pub fn list(&self) -> Vec<JobInfo> {
        self.lock()
            .jobs
            .iter()
            .map(|j| JobInfo { result: None, ..j.info.clone() })
            .collect()
    }

    /// Ask a job to stop. A queued job is cancelled at once; a running one
    /// stops its ffmpeg run and reports `cancelled` shortly after.
    pub fn cancel(&self, job_id: &str) -> Result<JobInfo, RpcError> {
        let mut inner = self.lock();
        let job = inner
            .jobs
            .iter_mut()
            .find(|j| j.info.job_id == job_id)
            .ok_or_else(|| RpcError::invalid_params(format!("no job `{job_id}`")))?;
        job.cancel.store(true, Ordering::SeqCst);
        let info = job.info.clone();
        drop(inner);
        // Wake a queued job so it notices.
        self.inner.1.notify_all();
        Ok(info)
    }

    /// Cancel everything (the daemon is stopping).
    pub fn cancel_all(&self) {
        for j in &self.lock().jobs {
            j.cancel.store(true, Ordering::SeqCst);
        }
        self.inner.1.notify_all();
    }

    /// `None` while a job is queued or running; else when the last one finished.
    pub fn idle_since(&self) -> Option<Option<Instant>> {
        let inner = self.lock();
        let busy = inner.jobs.iter().any(|j| !j.info.state.finished());
        (!busy).then_some(inner.last_finished)
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Drop the oldest finished jobs beyond [`KEEP_FINISHED`].
fn prune(inner: &mut Inner) {
    let mut finished = inner.jobs.iter().filter(|j| j.info.state.finished()).count();
    while finished > KEEP_FINISHED {
        let Some(i) = inner.jobs.iter().position(|j| j.info.state.finished()) else { break };
        inner.jobs.remove(i);
        finished -= 1;
    }
}

/// Change job `id`'s record (and the shared state) under the lock.
fn update(inner: &(Mutex<Inner>, Condvar), id: &str, f: impl FnOnce(&mut Inner, &mut JobInfo)) {
    let mut guard = inner.0.lock().unwrap_or_else(|e| e.into_inner());
    let g = &mut *guard;
    let Some(i) = g.jobs.iter().position(|j| j.info.job_id == id) else { return };
    // Take the record out so `f` can also touch the rest of `Inner`.
    let mut info = g.jobs[i].info.clone();
    f(g, &mut info);
    if let Some(j) = g.jobs.iter_mut().find(|j| j.info.job_id == id) {
        j.info = info;
    }
}

/// A job's thread: wait for a slot, run, record the outcome, tell subscribers.
fn run(
    inner: &Arc<(Mutex<Inner>, Condvar)>,
    events: &Events,
    id: &str,
    cancel: Arc<AtomicBool>,
    expected_secs: Option<f64>,
    work: impl FnOnce() -> Result<Value, RpcError>,
) {
    // Wait for a slot, unless cancelled while queued.
    let took_slot = {
        let mut g = inner.0.lock().unwrap_or_else(|e| e.into_inner());
        while g.running >= MAX_RUNNING && !cancel.load(Ordering::SeqCst) {
            g = inner.1.wait(g).unwrap_or_else(|e| e.into_inner());
        }
        let take = !cancel.load(Ordering::SeqCst);
        if take {
            g.running += 1;
        }
        take
    };
    let outcome = if took_slot {
        update(inner, id, |_, info| {
            info.state = JobState::Running;
            info.progress = expected_secs.map(|_| 0.0);
        });
        events.publish_job("job.progress", &json!({ "job_id": id, "state": "running", "progress": 0.0 }));
        let ctrl = progress_control(inner.clone(), events.clone(), id.to_owned(), cancel.clone(), expected_secs);
        control::scoped(Arc::new(ctrl), work)
    } else {
        Err(cancelled())
    };
    let (state, result, error) = match outcome {
        Ok(v) => (JobState::Done, Some(v), None),
        Err(e) => {
            // A cancel can surface as an ffmpeg failure (killed mid-write); the flag decides.
            let state = if e.code == ErrorCode::Cancelled || cancel.load(Ordering::SeqCst) { JobState::Cancelled } else { JobState::Failed };
            let e = if state == JobState::Cancelled { cancelled() } else { e };
            (state, None, Some(json!({ "code": e.code.rpc_code(), "message": e.message, "data": e.data })))
        }
    };
    update(inner, id, |g, info| {
        info.state = state;
        if state == JobState::Done {
            info.progress = Some(1.0);
        }
        info.finished_ms = Some(now_ms());
        info.result.clone_from(&result);
        info.error.clone_from(&error);
        g.last_finished = Some(Instant::now());
        if took_slot {
            g.running -= 1;
        }
        prune(g);
    });
    inner.1.notify_all();
    let mut params = json!({ "job_id": id, "state": state });
    if let Some(r) = result {
        params["result"] = r;
    }
    if let Some(e) = error {
        params["error"] = e;
    }
    events.publish_job("job.finished", &params);
}

/// The control a job's ffmpeg runs report to: progress goes into the job
/// record (for `jobs.get`) and out as `job.progress`, at most every
/// [`PROGRESS_EVERY`].
fn progress_control(
    inner: Arc<(Mutex<Inner>, Condvar)>,
    events: Events,
    id: String,
    cancel: Arc<AtomicBool>,
    expected_secs: Option<f64>,
) -> Control {
    let last = Mutex::new(None::<Instant>);
    Control::new(cancel, move |secs| {
        let Some(total) = expected_secs.filter(|t| *t > 0.0) else { return };
        // Never 1 before the job has really finished (muxing, a second pass).
        let fraction = ((secs / total).clamp(0.0, 0.99) * 1000.0).round() / 1000.0;
        update(&inner, &id, |_, info| info.progress = Some(fraction));
        let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY) {
            *last = Some(Instant::now());
            events.publish_job("job.progress", &json!({ "job_id": id, "state": "running", "progress": fraction }));
        }
    })
}

fn cancelled() -> RpcError {
    RpcError::new(ErrorCode::Cancelled, "cancelled")
}
