---
name: deepgram
description: Transcribe a video project's speech with Deepgram through the `ve` engine (`ve transcribe <asset>`), so you can read what was said and cut by words. Use when the user wants a transcript, wants to cut filler or a sentence out of a talking head or podcast, or asks what is said where.
---

# Deepgram speech to text

`ve transcribe <asset>` sends the asset's audio to Deepgram and stores the
words, with times, speakers and confidence, as the asset's transcript in the
project. It uses the user's own Deepgram account: it costs them money (a
few US cents per audio hour at the time of writing) and uploads their audio.
Say so before transcribing long footage they did not ask to transcribe.

## Setup (once)

```bash
ve plugin install plugins/deepgram          # from the ve repository, or a copy of this folder
ve plugin secret set deepgram DEEPGRAM_API_KEY
ve plugin list                              # shows "DEEPGRAM_API_KEY set (keychain)"
```

## Use

```bash
ve summary                                  # find the asset IDs
ve transcribe asset_94b8f2                  # detects the language; --language en to force it
ve transcribe asset_94b8f2 --keywords Gershon --keywords Kubernetes   # names it might mishear
ve transcript                               # what plays on the timeline, with timecodes
ve transcript --asset asset_94b8f2          # the whole file, in its own seconds
ve --json transcript                        # every word with start/end timecodes and clip IDs
```

Options: `--language`, `--model` (default `nova-3`), `--diarize false`
(speakers are told apart by default: `S0`, `S1`, ...), `--keywords`
(repeat it).

Transcribe each source once: the transcript is keyed by the file, kept in
the project's `transcripts/` folder, and follows every edit. Re-running
replaces it (and costs again).

## Cutting by words

`ve --json transcript` gives each word's `start` (the frame it starts in)
and `end` (the first frame after it). To remove words:

```bash
ve --actor agent:claude ripple-delete --from 00:00:12:04 --to 00:00:13:20
```

(the first word's `start` and the last word's `end`). Several cuts in one
step: `--range 00:00:12:04..00:00:13:20 --range 00:01:02:00..00:01:03:11`. Then run
`ve transcript` again: the times have moved, so always re-read before the
next cut, or make every cut of one pass in a single `ripple-delete` with
several `--range`s (all times from the same reading).
