# 0006 — alpha _CD
## Lantern Music Player - Build 260905

An interactive player for **0006 - alpha _CD** from **1000 Shrines of SPIRIT**. This is a single self-contained HTML file — no install, no build.

## Opening it

Save a local copy of the project, go to root directory, run `python3 -m http.server`, and visit the self-hosted url (usually `http://127.0.0.1:8000`).

Or just view the official online version at `https://johnoestmannmusic.com/1000-SOS/0006`

## Two playback modes

- **CHIP MODE** — plays the original Furnace-tracker-rendered stems, exactly as composed.
- **SAMPLER MODE** — replays the same song, but any instrument can be reassigned to play a real audio sample instead of the original chip sound. This is where most of the interactive sound design happens.

Switch between them with the pill toggle at the top of the page.

## The Sampler modal

Click **Sampler** on any instrument (in Sampler Mode) to open its editor:

- **Trim** — set exactly which part of the sample plays, by dragging on the waveform or typing Start/End.
- **Loop**, including **Ping-pong** (bounces forward/backward instead of restarting).
- **ADSR envelope** — attack/decay/sustain/release, editable by dragging points on the graph.
- **Transpose** — pitch-shift the sample, in semitones and cents.
- **Volume** and **Random Pan** — per-instrument level, plus optional pan variation on every note.
- **Spectral Fusion** — morph the sample with a second sample using one of six algorithms (Freeze, Cross-Synth, Convolve, Ring Modulate, Freq Shift, Smear), with an optional Formant Shift on top. This is where the more extreme, unusual sounds come from.

Everything previews live as you adjust it.

## Changing Samples
The program reads in samples, named `0.ogg` - `5.ogg` (max 6 files) in the `ASSETS/SourceSamples/` directory.

You can `Load` samples via the GUI for testing, though due to browser security, this won't write to disk. You can click the `Package Samples` button to download the loaded ones as pre-named WAVs. These will need to be converted, named `0.ogg` - `5.ogg` and put in the `ASSETS/SourceSamples/` directory. The `wav2ogg.py` script there can convert up to 6 WAV files in that directory to correctly named OGGs.

## Saving and loading your changes

The **Project JSON** button exports everything you've set up — mixer levels, every instrument's Sampler and Spectral Fusion settings, loaded samples, song metadata — as a block of JSON. Copy it out to keep a version of your setup, or paste a previously copied one back in and hit Confirm to restore it.

The JSON that is loaded on page open is `ASSETS/lmp-default-proj.json`. Overwrite this with your changes.

## License

- Music License: CC0 / Public Domain
- Code License: MIT
