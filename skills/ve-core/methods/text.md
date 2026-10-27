# Text and captions

Text clips are titles, labels, lower thirds and captions drawn by the
engine. A text clip is an ordinary clip on a video track: split, trim,
move, delete, transforms (position, scale, rotation, opacity, blend,
keyframes), effects and transitions all work on it. It is stored as an
OTIO `GeneratorReference` with `generator_kind: "ve.text"` and the text and
style as `parameters`, so other OTIO tools see a generated clip, not
missing media.

Text is drawn by the GPU compositor only. A render through the ffmpeg path
(no GPU) leaves text out and says so in `warnings`.

Fancy animated titles (HTML, 3D) are not core: they come from generator
plugins, and a text clip can sit next to them inside a composite clip.

## Style

Every field has a default, so `{}` is plain white text. Sizes are fractions
of the canvas, so a clip looks the same at any render size.

| Field | Default | |
| --- | --- | --- |
| `font` | `sans-serif` | Family name (`"Bebas Neue"`, `"Arial"`), or `sans-serif`, `serif`, `monospace`. Project fonts ([Fonts](#fonts)) or installed ones; an unknown family falls back to a sans-serif |
| `size` | 0.07 | Letter size: the font's em as a fraction of the canvas height (0.005 to 1) |
| `weight` | 400 | 100 (thin) to 900 (black); 700 is bold |
| `italic` | false | |
| `color` | `[1, 1, 1, 1]` | sRGB `[r, g, b]` or `[r, g, b, a]`, 0 to 1 |
| `align` | `center` | `left`, `center`, `right`, within the text box |
| `width` | 0.8 | Longest line before wrapping, as a fraction of the canvas width |
| `line_height` | 1.2 | Line spacing, as a multiple of `size` |
| `outline` | 0 | Outline thickness as a fraction of `size` (0.1 is a solid caption outline) |
| `outline_color` | `[0, 0, 0, 1]` | |
| `shadow` | 0 | Drop shadow offset, down and right, as a fraction of `size` |
| `shadow_color` | `[0, 0, 0, 0.6]` | |
| `background` | `[0, 0, 0, 0]` | Box behind the text; alpha 0 draws none |
| `padding` | 0.3 | Space between the text and the box edge, as a fraction of `size` |
| `highlight` | none | Color of the word being spoken, for clips with word timings ([Animations](#animations)) |

**Placement.** The picture is the text box (text plus padding), centered on
the canvas. Move it with `x` / `y` on `text.add`, or with the clip's
transform (`edit.set_transform`): `x` in canvas widths from the center,
`y` in canvas heights, positive down (`y: 0.35` sits low, `-0.35` high).
`scale` scales around the box's center and stays sharp.
`render.layers` reports a text clip's words as `text`; text never hides
what is under it.

## Animations

A text clip can have an entrance (`animate_in`) and an exit
(`animate_out`): `{"kind": "pop", "duration": "0.4s"}` (duration defaults
to 0.4 s and is at most half the clip). They play at the clip's start and
end, follow trims, and add to the clip's transform (a sliding title can
still sit in a corner).

| `kind` | Entrance (an exit plays it backwards) |
| --- | --- |
| `fade` | Opacity from 0 |
| `pop` | Grows from half size with a little overshoot |
| `zoom` | Shrinks into place from 1.4x |
| `slide-up`, `slide-down`, `slide-left`, `slide-right` | Moves into place from a little away, fading in |
| `typewriter` | Letters appear one after another |
| `words` | Words appear one after another |

Letters and words still to come keep their place, so nothing shifts as
they appear. `render.layers` reports the animated opacity and rectangle.

**Word highlight.** A text clip with word timings (`words`, which
`text.captions` fills in from the transcript) and a `highlight` color in
its style draws the word being spoken in that color, the CapCut style:
`text.captions` with `"highlight": [1, 0.85, 0]`.

## Fonts

A style's `font` names a family from the project's own fonts or the ones
installed on the machine. Project fonts live in `<project>/fonts/` and
travel with the project, so it renders the same anywhere (a server has
almost no installed fonts). Adding or replacing a file there while the
engine runs is picked up within a second; cached frames never show an old
font.

## `font.add`

Give one of:

| Param | |
| --- | --- |
| `path` | A font file on this machine (`.ttf`, `.otf`, `.ttc`, `.otc`) |
| `url` | A font file to download |
| `google` | A Google Fonts family by name (`"Bebas Neue"`): every weight and italic it has, as TrueType files |
| `replace` | Overwrite a file of the same name with other content (else refused) |

```json
{"files": [{"file": "BebasNeue-400.ttf", "faces": [{"family": "Bebas Neue", "weight": 400, "italic": false}]}],
 "families": ["Bebas Neue"]}
```

`existing: true` marks a file that was already there with the same
content. A file that is not a font is refused before anything is written.
Google Fonts and URLs need internet access; WOFF/WOFF2 web fonts are not
read, but Google Fonts sends TrueType to the engine.

## `font.list`

`{"system": true}` also lists installed families. Result: `{"project":
[{"file", "faces"}], "system": ["Arial", ...]}`. A project using a system
font may look different on a machine without it.

## `font.remove`

`{"file": "BebasNeue-400.ttf"}` deletes that file from `fonts/`
(`removed: false` when it was not there). Text using the family falls back
to a sans-serif.

## `text.add` (edits)

| Param | Default | |
| --- | --- | --- |
| `text` | required | The words. `\n` starts a new line; long lines wrap at `width` |
| `at` | timeline start | Timeline start |
| `duration` | 3 s | Length |
| `track` | | A video track by name or ID. Without it: the topmost video track that is free over the clip, else a new video track `Text` on top |
| `overwrite` | false | With `track`: replace what is under the clip (else refused when the track is busy there) |
| `style` | `{}` | Fields from the table above |
| `x`, `y` | 0 | Center position (a transform with only x/y) |
| `animate_in`, `animate_out` | | Entrance and exit ([Animations](#animations)) |
| + `base_rev`, `dry_run`, `actor` | | |

Text never pushes other clips. The result is an edit report:

```json
{"revision": 5, "changed": true, "dry_run": false, "patch": [...],
 "created": [{"id": "clip_528d", "track": "Text", "start": "00:00:01:00", "end": "00:00:03:00"}],
 "duration": "00:10:02:15"}
```

## `text.set` (edits)

| Param | | |
| --- | --- | --- |
| `clip` | required | A text clip |
| `text` | | New words |
| `style` | | Fields to change; `null` puts one back to its default |
| `reset_style` | false | Start from the default look before applying `style` |
| `animate_in`, `animate_out` | | New entrance or exit; `null` removes it |
| + `base_rev`, `dry_run`, `actor` | | |

New `text` drops the old word timings. Unknown style fields are refused, with the list of fields. Position, size
on screen, rotation, opacity and blend belong to the transform:
`edit.set_transform`.

## `text.captions` (edits)

Caption clips from the transcript ([transcript.md](transcript.md)), timed
to the edited timeline: one text clip per caption, grouped like the SRT
export (at most two 42-character lines and 6 seconds; a new caption on a
speaker change, a pause of 0.7 s or more, or after a sentence).

| Param | Default | |
| --- | --- | --- |
| `from`, `to` | whole timeline | Timeline range |
| `track` | | A video track: its text clips in the range are replaced first, so running it again after editing refreshes the captions. Default: a new `Captions` track on top (or the topmost video track free over every caption) |
| `style` | | Fields over the caption default: bold, size 0.055, outline 0.12, width 0.85 |
| `y` | 0.36 | Vertical position (near the bottom) |
| `highlight` | | Color of the word being spoken, e.g. `[1, 0.85, 0]` |
| `animate_in` | | Entrance for every caption, e.g. `{"kind": "pop", "duration": "0.15s"}` |
| + `base_rev`, `dry_run`, `actor` | | |

```json
{"revision": 9, "changed": true, "dry_run": false, "patch": [...],
 "created": [{"id": "clip_a1", "track": "Captions", "start": "00:00:00:02", "end": "00:00:01:08"}, ...],
 "removed": [], "duration": "00:10:02:15", "captions": 112, "missing": []}
```

`missing` lists assets on audio tracks in the range with no transcript yet;
they get no captions. Captions are clips, so later ripple edits move them
with everything else; after cutting words, run `text.captions` again with
`track` to retime them from the transcript.

For a caption file instead of burned-in text: `transcript.get` with
`format: srt` or `vtt`.
