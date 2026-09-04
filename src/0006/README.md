# 0006 — alpha _CD

An interactive player for one track from **1000 Shrines of SPIRIT**. This is a single self-contained HTML file — no install, no build, no server required.

## Opening it

Just open `index.html` in a browser, or visit the hosted page. Everything (player, music data, and visuals) lives in that one file.

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

## Saving and loading your changes

The **Project JSON** button exports everything you've set up — mixer levels, every instrument's Sampler and Spectral Fusion settings, loaded samples, song metadata — as a block of JSON. Copy it out to keep a version of your setup, or paste a previously copied one back in and hit Confirm to restore it.

## Source and links

The footer links to the full source code and the artist's website.

- Music License: CC0 / Public Domain
- Code License: MIT
