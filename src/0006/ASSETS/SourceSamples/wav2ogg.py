#!/usr/bin/env python3
"""Converts .wav files in this folder into numbered .ogg files (0.ogg,
1.ogg, ...) for flight_school_night_shift.html's Source Sample slots.

Files are numbered by position — the first .wav (sorted by filename)
becomes 0.ogg, the second becomes 1.ogg, and so on — up to 6, since the
page only has 6 slots. If any target .ogg already exists, asks once
whether to overwrite; a "yes" overwrites all of them, a "no" aborts
without changing anything. Requires ffmpeg on PATH.
"""
import shutil
import subprocess
import sys
from pathlib import Path

SLOTS = 6
HERE = Path(__file__).parent


def main():
    if not shutil.which('ffmpeg'):
        sys.exit('ffmpeg not found on PATH — install it (e.g. apt/brew install ffmpeg) and try again.')

    wavs = sorted(set(HERE.glob('*.wav')) | set(HERE.glob('*.WAV')))
    if not wavs:
        print('No .wav files found here — nothing to do.')
        return
    if len(wavs) > SLOTS:
        print(f'Found {len(wavs)} .wav files; only converting the first {SLOTS}.')
    wavs = wavs[:SLOTS]

    targets = [(i, wav, HERE / f'{i}.ogg') for i, wav in enumerate(wavs)]
    existing = [out for _, _, out in targets if out.exists()]
    if existing:
        names = ', '.join(o.name for o in existing)
        reply = input(f'{len(existing)} file(s) already exist and will be overwritten ({names}). Continue? [y/N] ')
        if reply.strip().lower() not in ('y', 'yes'):
            print('Aborted — nothing was changed.')
            return

    for slot, wav, out in targets:
        print(f'{wav.name} -> {out.name}')
        subprocess.run(
            ['ffmpeg', '-y', '-loglevel', 'error', '-i', str(wav), '-c:a', 'libvorbis', '-q:a', '5', str(out)],
            check=True,
        )

    print(f'Done — {len(targets)} file(s) converted.')


if __name__ == '__main__':
    main()
