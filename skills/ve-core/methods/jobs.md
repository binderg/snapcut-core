# jobs.*

Long work can run in the background. These methods take `background: true`:
`render.export`, `render.range_preview`, `render.contact_sheet`,
`analyze.silence`, `analyze.scenes`, `measure.loudness`, `measure.levels`,
`measure.waveform`, `measure.pitch` and `edit.normalize`. Called that way,
they answer at once:

```json
{"job_id": "job_3f2a91", "state": "queued"}
```

and the work runs in the daemon. Without `background` they behave as before:
the call returns when the work is done.

Either way, slow work no longer holds up other calls. It runs on a copy of
the project taken when the call arrives, so frames, edits and summaries keep
answering during an export. An export renders the timeline as it was when it
started; edits made meanwhile are not in that file.

## States

`queued` → `running` → `done` | `failed` | `cancelled`

At most **2 jobs run at once**; more wait as `queued`. Jobs live in the
daemon's memory: if the daemon stops, running jobs are cancelled and the
list is gone. The last 50 finished jobs are kept.

## `jobs.get`

`{job_id}`. The job, with its result once done:

```json
{"job_id": "job_3f2a91", "method": "render.export", "state": "running", "progress": 0.42,
 "created_ms": 1761090000000}
```

```json
{"job_id": "job_3f2a91", "method": "render.export", "state": "done", "progress": 1.0,
 "created_ms": 1761090000000, "finished_ms": 1761090265000,
 "result": {"path": "...export-rev12-1761090000.mp4", "frames": 18075, "...": "..."}}
```

- `progress` is 0 to 1, from ffmpeg's position in the media. It stays below 1
  until the job is really done (an export still has to finish writing the
  file). Absent when the job cannot tell how long its work is.
- `result` is exactly what the method returns when called without
  `background`.
- `error` (failed or cancelled) is the JSON-RPC error object: `code`,
  `message`, `data`. Cancelled jobs have code `-31004` (`cancelled`).

## `jobs.list`

`{}`. `{"jobs": [...]}`, oldest first, each as in `jobs.get` but **without
`result`** (it can be large: a contact sheet is a whole image). Fetch one with
`jobs.get`.

## `jobs.cancel`

`{job_id}`. Stops the job: a queued one at once, a running one within a
fraction of a second (its ffmpeg run is killed). Returns the job as it was
when asked; the state turns `cancelled` shortly after. Cancelling a finished
job changes nothing.

## Events

Connections that called `events.subscribe` get, for every job:

```json
{"jsonrpc": "2.0", "method": "job.progress", "params": {"job_id": "job_3f2a91", "state": "running", "progress": 0.42}}
{"jsonrpc": "2.0", "method": "job.finished", "params": {"job_id": "job_3f2a91", "state": "done", "result": {"...": "..."}}}
```

`job.progress` comes at most 4 times a second per job. `job.finished` comes
once, with `result` or `error`. Events go to every subscriber, not only the
caller that started the job; match on `job_id`. Without a subscription, poll
`jobs.get` (a few times a second is fine).

## Jobs that edit

`analyze.*` and `edit.normalize` change the project at the end (markers, clip
gain). They check `base_rev` when they start and again when they commit. If
anyone edited the project while the job ran, the job fails with
`stale_revision`: its findings were computed on the old timeline and would
land in the wrong places. Run it again on the new revision.

## From the CLI

`ve export`, `ve preview`, `ve sheet`, `ve analyze ...`, `ve measure ...` and
`ve normalize` run as jobs: a progress line on stderr (only when stderr is a
terminal), and Ctrl+C cancels the job and exits with code 130. `ve jobs`
lists jobs, `ve jobs get <id>` shows one, `ve jobs cancel <id>` stops one.
