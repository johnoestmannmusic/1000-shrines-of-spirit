# Lantern Music Player

Lantern Music Player is a self-contained web page that plays back a Furnace
4-channel Game Boy track and lets you explore it while it plays — patterns, instruments,
piano roll, timing, and an animated cover-art visualizer, all hover/click-
driven. `lantern-build.py` is what turns a raw Furnace export into that finished
page.

## Requirements

- Python 3
- [ffmpeg](https://ffmpeg.org/) available on your `PATH` (used to encode the
  playback stems to `.ogg` and to peak-normalize the downloadable `.wav`)

## Workflow

**1. Export the song from Furnace**, all under the same base filename (e.g.
`mysong`):

- `mysong.json` — Furnace's JSON export (as of 30 Aug 26, this is only in the nightly builds) (File → Export → export as .json) —
  this is the actual data source for everything on the page.
- `mysong.txt` — Furnace's plain-text export (File → Export → export as
  text) — used only as a sanity check against the JSON.
- `mysong.fur` — the Furnace module itself (i.e. just the saved project
  file) — shipped as a download on the page.
- `mysong.wav` — the full mixed song, exported to one stereo `.wav` —
  shipped as a download (peak-normalized during the build, see below).
- `0.wav`, `1.wav`, `2.wav`, `3.wav` — the four channels exported as
  separate stems, in channel order — these are what the page actually plays
  back in sync while you interact with it. I do this by, in Furnace, soloing each channel, and exporting the WAV, making sure loop and fadeout are set to 0.

**2. Put all 8 files in `/INGEST/`.** The four with a shared
base name (`mysong.*`) must genuinely share that exact name — that's how
the build script knows the track's name. If you see a mismatch (e.g. one
file saved with a hyphen and another with an underscore), rename until they
match.

**3. Run the build:**

In the root directory (not in the `INGEST` folder):

```
python3 lantern-build.py
```

No arguments — it always works on whatever's in its own folder's `/INGEST/`.
If anything's missing it prints a checklist telling you exactly what it did
and didn't find, and stops.

**4. Check the output.** A successful build produces:

- `/ASSETS/` — the four `.ogg` stems, `mysong.fur`, `mysong.wav` (peak-
  normalized to −0.1 dB true peak), and `mysong.mid` (a Standard MIDI File
  generated straight from the song data)
- `mysong.html` — the finished, playable page. Open it in a browser to
  check playback, the patterns/instruments panels, and that the three
  download buttons (⇩ .fur / ⇩ export MIDI / ⇩ WAV) all work.

Along the way it also prints a sanity check comparing the `.txt` export
against the `.json` export (song info + instrument list) — a mismatch there
usually means the two files came from different exports and one is stale.
It's a warning, not a hard stop.

## Swapping in a different track

To replace the song entirely, just clear out `INGEST/` and drop in a new
set of the seven files above (any track name), then run
`python3 lantern-build.py` again. The build script finds whatever shrine
page already exists here, uses it as the template, and retires the old one
automatically (backing it up first) — since `ASSETS/` always holds exactly
one track's files, the old page would otherwise be left pointing at the new
track's audio.

## Re-running on the same track

If you just re-export an updated version of the same song (same trackname),
running `lantern-build.py` again simply regenerates everything from
scratch — safe to run as many times as you like.
