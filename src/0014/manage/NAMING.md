# NAMING.md — attribution & naming policy

0012 must be an **original** work. No third-party product, brand, preset-patch or
game name may appear in source, comments, documentation, UI, recipe codes, or
the emitted artifact. This is enforced as a blocking rule (`HC002` in
`CONSTRAINTS.md`) and can be checked by hand with `scripts/naming-audit.sh`.

**The one rule: describe the synthesis, never the gear.**

- ✅ "an FM bell over a warm swelling pad", "two-operator FM", "a noise bed"
- ❌ "a D-50 Fantasia pad", "DX-style glass", "Wavestation wave sequence"

Generic English words are fine: *wind, glass, stream, breath, haze, shimmer*.
Brand names, product model numbers, and famous preset-patch names are not.

---

## Enforced denylist

Scanned by `CONSTRAINTS.md` (scope: `engine/**`, `src/**`, `web/**`) and by
`scripts/naming-audit.sh`:

```
rain world, fantasia, roland, korg, yamaha, moog, oberheim, novation, clavia,
nord, wavestation, mellotron, fairlight, ensoniq, jd-800, rhodes, dx7 / dx-7, d-50, tb-303,
tr-808, tr-909, prophet-5, juno-106, jupiter-8, sequential circuits,
access virus, native instruments, omnisphere, spectrasonics, kontakt,
arturia, sylenth, absynth, reaktor
```

Add new terms here and in `CONSTRAINTS.md` together when they are discovered.

---

## Rename table (0011 → 0012)

The 0011 engine we port from uses preset-derived names. Use the right-hand
column in 0012 — display name **and** recipe code. The descriptions must also
drop the gear references (DX / D-50 / Wavestation).

| 0011 display (code) | 0012 display (code) | 0012 description |
|---|---|---|
| Choir "Aah" (`CHO`) | Voices (`VOX`) | stacked detuned voices whose vowel drifts a → o → u → e |
| Glass (`GLS`) | Bells (`BEL`) | two-operator FM bell, struck softly in a slow pattern |
| Fantasia (`FAN`) | Bell Pad (`BLP`) | a bright FM bell over a warm, swelling pad |
| WaveSeq (`WAV`) | Wave Cycle (`WVC`) | the waveform changes every beat |
| Breath (`AIR`) | Air (`AIR`) | noise tuned to the chord: airy, whispered |
| Whisper (`WHI`) | Hush (`HSH`) | noise shaped by gliding vowel formants: a wordless breath |
| Wind (`WND`) | Gust (`GST`) | noise through a slowly opening low-pass: gusts and calm |
| Shimmer (`SHM`) | Shine (`SHN`) | high, drifting resonances: air sparkling in the light |
| Stream (`STR`) | Rill (`RIL`) | narrow bands fluttering quickly: water over stones |
| Aurora (`AUR`) | Veil (`VEL`) | resonant bands sweeping up and back: a curtain of light |
| Haze (`HAZ`) | Mist (`MST`) | a low, slowly breathing noise bed with almost no pitch |
| Off (`OFF`) | Off (`OFF`) | no loop layer |

Later loop timbres are 0012's own from the start (no 0011 counterpart), named
for what they sound like and checked against the denylist:

| 0012 display (code) | description |
|---|---|
| Drift (`DRF`) | detuned pulse-wave strings, slowly chorused and breathing |
| Tide (`TDE`) | a bright saw pad through a slowly sweeping notch |
| Ember (`EMB`) | a dark pad pulsing through a soft rhythmic gate |
| Ripple (`RPL`) | plucked strings arpeggiating the chord, with a dotted echo |
| Ebb (`EBB`) | reversed swells that breathe in backwards, then stop |
| Bronze (`BRZ`) | inharmonic gong partials swelling and beating slowly |
| Frost (`FRS`) | tiny high grains scattered over the chord |
| Lantern (`LNT`) | a breathy flute-organ with a chiff and slow tremolo |
| Loom (`LOM`) | a soft square-wave arpeggio through an opening filter |

> These recipe codes are 0012's own; they need not match 0011's recipes. 0011 is
> a separate, already-shipped project and is intentionally left untouched.

Kits (`Acoustic`, `FM metal`, `Modal`, `Crush`, `Noise sculpture`, `Sub &
clicks`, `Micro blips`, `Glass pings`, `Data dust`, `Pulse code`, `Off`) are
descriptive and generic; no change required.

---

## Checklist before a card is marked Implemented

1. `scripts/naming-audit.sh` exits 0 over `engine src web`.
2. `constraints_run_tests` reports `HC002` green.
3. New instruments have descriptive names and gear-free descriptions.
