# transcript.*

A transcript is what was said in one asset's audio, word by word. The core
owns the data; **making** a transcript is a plugin's job (speech to text,
local or cloud: see [plugin.md](plugin.md)). Anyone can store one with
`transcript.set`, so a transcript from another tool works too.

- A transcript belongs to an **asset**, and its times are **media times** of
  that asset's file. Cuts, trims and speed changes never make it stale.
  `transcript.get` without an asset maps the words onto the timeline.
- It is kept in the project folder (`transcripts/<file hash>.json`): it is
  user data that cost money or minutes to make, not a cache. Keyed by the
  file's content, it survives removing and re-importing the asset.
- Word times are exact decimal seconds kept to the millisecond:
  `"12.345s"`, the same seconds form every time param accepts
  ([../model/time.md](../model/time.md)). Never numbers.

Word shape:

| Field | Type | |
| --- | --- | --- |
| `start` | string | Media time the word starts (`"1.230s"`) |
| `end` | string | Media time it ends, exclusive |
| `text` | string | The word as it should read, punctuation included if the model gives it |
| `speaker` | string? | Speaker label (`"S0"`, `"S1"`) when the model separates speakers |
| `confidence` | number? | 0 to 1, when the model reports it |

## `transcript.set`

| Param | Type | |
| --- | --- | --- |
| `asset` | string | The asset the words belong to |
| `words` | [word] | Every word (any order; stored sorted by start) |
| `language` | string? | BCP 47 tag (`en`, `he`) |
| `speakers` | [{id, name?}]? | Display names for speaker labels |
| `provenance` | object? | Who made it: plugin, model, params, request id. Free form |
| `actor` | object? | Recorded in `provenance.actor` (a plugin's calls carry `plugin:<id>` automatically) |

Replaces any earlier transcript of the same file. Not an edit: no revision
change, no undo (the timeline does not change). Refused (`invalid_params`)
when a word ends before it starts, starts before 0, has a confidence outside
0..1, or ends more than half a second past the end of the file.

```json
{"asset": "asset_94b8f2", "words": 1873, "end": "602.410s", "replaced": false}
```

## `transcript.get`

| Param | Type | |
| --- | --- | --- |
| `asset` | string? | One asset's whole transcript, in its media time |
| `from`, `to` | time? | Without `asset`: the timeline range (default: all) |
| `format` | `words` \| `text` | Default `words`. `text` gives readable lines instead |

**With `asset`**: the stored transcript, with `language`, `speakers`,
`provenance` and `words` (or `text`). No transcript yet -> `invalid_params`.

**Without `asset`**: the words that play on the timeline, from every audio
clip whose asset has a transcript, in timeline order:

```json
{"revision": 12,
 "words": [
   {"start": "00:00:00:02", "end": "00:00:00:13", "text": "Hello", "speaker": "S0",
    "confidence": 0.98, "clip": "clip_a40e61", "asset": "asset_94b8f2", "source_start": "0.080s"}
 ],
 "missing": ["asset_77c1d0"]}
```

- `start` is the timeline frame the word starts in; `end` is the first
  frame after it (rounded up). So `edit.ripple_delete` with
  `{"from": start, "to": end}` removes the whole word.
- A word belongs to a clip when its middle falls inside the part of the
  media the clip plays; a word cut in half by an edit is shown clipped to
  the clip.
- Speed changes are accounted for (a word in a 2x clip takes half the time).
- `missing` lists assets on audio tracks in the range that have no
  transcript yet.

`format: text` groups words into lines: a new line on a speaker change, a
pause of a second or more, after a sentence once a line has 8 words, or at
20 words. Lines start with the timecode (or, with `asset`, the media time):

```
[00:00:00:02] S0: Hello world. Thanks for having me.
[00:00:02:03] S1: Glad you could make it!
```

This is the cheapest way for an agent to see a talking head: read the text,
find the words to cut, then use the word list for exact times.

## `transcript.list`

`{}`. Assets with a transcript:

```json
{"transcripts": [{"asset": "asset_94b8f2", "words": 1873, "end": "602.410s", "language": "en",
                  "provenance": {"plugin": "deepgram", "model": "general-nova-3", "actor": "plugin:deepgram"}}]}
```

## `transcript.remove`

`{"asset": "asset_94b8f2"}` -> `{"asset": "asset_94b8f2", "removed": true}`
(`false` when there was none). Deletes the file in `transcripts/`.
