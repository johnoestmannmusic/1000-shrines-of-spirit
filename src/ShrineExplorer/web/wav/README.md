# Bundled audio

Drop the rendered track in this folder (any `.wav` name) and it ships with the
build:

    web/wav/<your render>.wav

`scripts/build-dist.sh` copies `web/wav/*.wav` into `dist/wav/` and writes a
`dist/wav/manifest.json` listing them; `cargo run -- --serve` generates the same
listing on the fly. The page reads that listing and offers a **Download WAV**
button beside the recipe, so the file keeps whatever name you gave it. With no
WAV present the button stays hidden and the page is unchanged.

Render one from the setup menu (**Export WAV**, which writes a 48 kHz stereo WAV
with the track name, artist and album as metadata and the 1600×1600 cover
embedded as cover art), then copy it here. The track name set in the menu is used
as the WAV title.
