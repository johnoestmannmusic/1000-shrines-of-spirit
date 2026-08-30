#!/usr/bin/env python3
"""
Regenerates this shrine's SPA from a raw Furnace export dropped in INGEST/.

Usage: python3 build.py   (no arguments — always operates on the folder
this script lives in)

Expects src/0005/INGEST/ to contain:
  <trackname>.json   Furnace's JSON export (the core data source)
  <trackname>.txt    Furnace's plain-text export (used as a sanity check)
  <trackname>.fur    the original Furnace module (shipped as a download)
  <trackname>.wav    the full mixed song (shipped as a download)
  0.wav .. 3.wav      the four per-channel stems

Produces:
  ASSETS/0.ogg .. 3.ogg, ASSETS/<trackname>.fur, ASSETS/<trackname>.wav,
    ASSETS/<trackname>.mid (generated here — see build_midi_bytes() below,
    a port of the buildMidiBytes() that used to run in-browser)
  <shrine-folder>-<trackname>.html   (e.g. 0005-flight_school.html)
  <shrine-folder>-<trackname>-backup.html  (previous version, if any)
"""
import datetime
import json
import math
import os
import re
import shutil
import subprocess
import sys
from html import escape as html_escape

GITHUB_REPO_URL = "https://github.com/johnoestmannmusic/1000-shrines-of-spirit"
FURNACE_URL = "https://github.com/tildearrow/furnace"
COMPOSER_SITE_URL = "https://johnoestmannmusic.com"
WAV_PEAK_NORMALIZE_DB = -0.1

SHRINE_DIR = os.path.dirname(os.path.abspath(__file__))
INGEST_DIR = os.path.join(SHRINE_DIR, "INGEST")
ASSETS_DIR = os.path.join(SHRINE_DIR, "ASSETS")


def fail(msg):
    print(msg, file=sys.stderr)
    sys.exit(1)


def find_trackname():
    if not os.path.isdir(INGEST_DIR):
        fail("ERROR: no INGEST/ folder found at %s\n"
             "Create src/0005/INGEST/ and populate it with the files listed "
             "in this script's docstring." % INGEST_DIR)
    json_files = [f for f in os.listdir(INGEST_DIR) if f.lower().endswith(".json")]
    if len(json_files) == 0:
        fail("ERROR: no <trackname>.json found in INGEST/ — that's the file "
             "build.py uses to determine the track name.")
    if len(json_files) > 1:
        fail("ERROR: found more than one .json file in INGEST/ (%s) — "
             "INGEST/ should hold exactly one song's export files."
             % ", ".join(json_files))
    return os.path.splitext(json_files[0])[0]


def check_required_files(trackname):
    required = [
        trackname + ".json",
        trackname + ".txt",
        trackname + ".fur",
        trackname + ".wav",
        "0.wav", "1.wav", "2.wav", "3.wav",
    ]
    print("INGEST/ checklist (trackname = %r):" % trackname)
    missing = []
    for name in required:
        path = os.path.join(INGEST_DIR, name)
        ok = os.path.isfile(path)
        print("  [%s] %s" % ("x" if ok else " ", name))
        if not ok:
            missing.append(name)
    if missing:
        fail(
            "\nERROR: INGEST/ is missing %d file(s) listed above with an empty "
            "checkbox.\nAll four of <trackname>.json/.txt/.fur/.wav must share "
            "the exact same basename (%r here) — check for a naming mismatch "
            "(e.g. hyphen vs. underscore) if a file you expect is listed as "
            "missing." % (len(missing), trackname)
        )
    print()


# --- Stage 2: load JSON, sanity-check against the .txt export -------------

def parse_txt_sanity_fields(txt_path):
    with open(txt_path, "r", encoding="utf-8") as f:
        lines = f.read().splitlines()

    song_info = {}
    instruments = []
    section = None
    current_ins = None
    for line in lines:
        heading = re.match(r"^#\s+(.*)", line)
        if heading and not line.startswith("##"):
            section = heading.group(1).strip()
            current_ins = None
            continue
        if section == "Song Information":
            m = re.match(r"^-\s*([\w /]+?):\s*(.*)$", line)
            if m:
                song_info[m.group(1).strip().lower()] = m.group(2).strip()
        elif section == "Instruments":
            m = re.match(r"^##\s*([0-9A-Fa-f]+):\s*(.*)$", line)
            if m:
                current_ins = {"index": int(m.group(1), 16), "name": m.group(2).strip()}
                instruments.append(current_ins)
    return song_info, instruments


def sanity_check(data, txt_path):
    song_info, txt_instruments = parse_txt_sanity_fields(txt_path)
    j = data.get("songInfo", {})
    mismatches = []

    def cmp_field(txt_key, json_value, label=None):
        label = label or txt_key
        txt_value = song_info.get(txt_key)
        if txt_value is None:
            return
        if str(json_value) != str(txt_value):
            mismatches.append((label, txt_value, json_value))

    cmp_field("name", j.get("name"))
    cmp_field("author", j.get("author"))
    cmp_field("album", j.get("album"))
    system = j.get("system")
    cmp_field("system", system[1] if isinstance(system, list) and len(system) > 1 else system)
    cmp_field("instruments", j.get("instrumentCount"), label="instrument count")
    cmp_field("wavetables", j.get("wavetableCount"), label="wavetable count")
    cmp_field("samples", j.get("sampleCount"), label="sample count")

    json_instruments = data.get("instruments", [])
    if len(txt_instruments) != len(json_instruments):
        mismatches.append(("instrument list length", len(txt_instruments), len(json_instruments)))
    else:
        for txt_ins, json_ins in zip(txt_instruments, json_instruments):
            json_name = (json_ins or {}).get("name", "")
            if txt_ins["name"] != json_name:
                mismatches.append(
                    ("instrument %02X name" % txt_ins["index"], txt_ins["name"], json_name)
                )

    if mismatches:
        print("SANITY CHECK: %d mismatch(es) between the .txt export and the .json export:" % len(mismatches))
        for label, txt_value, json_value in mismatches:
            print("  \u26a0 %s: txt=%r json=%r" % (label, txt_value, json_value))
        print("(non-fatal — continuing the build; double-check these aren't from two different exports)\n")
    else:
        print("SANITY CHECK: .txt export matches .json export on song info + instrument list.\n")


# --- Stage 2.5: build a Standard MIDI File -------------------------------
#
# This is a straight line-by-line Python port of the readFurModule /
# buildSongModel / walkChannelEvents / buildInstrumentTrackEvents /
# buildMidiBytes functions that used to live in the shipped HTML's
# <script> and run in-browser on every "export MIDI" click. Moved here so
# it runs once at build time instead — the shipped page just downloads the
# resulting static ASSETS/<trackname>.mid now (see downloadAsset() in
# regenerate_html below). Verified byte-for-byte identical to what the old
# in-browser version produced (see build.py's own test notes / commit).

NOTE_OFF = 253
A_REF_NOTE = 60 + 5 * 12 + 9  # Furnace's own octave numbering vs. MIDI/scientific pitch — see the (removed) JS comment this was ported from.
MIDI_TICKS_PER_AUDIO_TICK = 4
PITCH_SLIDE_UNIT = 1.0 / 128
VIBRATO_DEPTH_UNIT = 1.0 / 15
VIBRATO_SPEED_UNIT = 4
MIDI_BEND_RANGE_SEMITONES = 12


def js_round(x):
    """Math.round semantics (round-half-up, including negatives) — Python's
    built-in round() uses round-half-to-even, which disagrees on .5 values."""
    return math.floor(x + 0.5)


def read_fur_module(j):
    return {
        "songInfo": j["songInfo"],
        "chips": j["chips"],
        "instruments": j["instruments"],
        "wavetables": j["wavetables"],
        "subsong": j["subsongs"][0],
    }


def build_song_model(mod):
    ss = mod["subsong"]
    channels = []
    for ch_idx, cd in enumerate(ss["channelData"]):
        channels.append({
            "index": ch_idx,
            "effectColumns": cd["effectColumns"],
            "orderList": list(ss["orders"][ch_idx]),
            "patterns": ss["patterns"][ch_idx],
        })
    song_info = mod["songInfo"]
    system = song_info.get("system")
    return {
        "meta": {
            "name": song_info.get("name"),
            "author": song_info.get("author"),
            "album": song_info.get("album"),
            "system": system[1] if system else None,
            "tuning": song_info.get("tuning"),
            "formatVersion": song_info.get("version"),
            "comments": song_info.get("comments") or "",
            "tickRate": ss["tickRate"],
            "speeds": ss["speeds"],
            "virtualTempo": ss["virtualTempo"],
            "highlights": ss["highlights"],
            "orderLength": ss["orderLength"],
            "patternLength": ss["patternLength"],
        },
        "chips": mod["chips"],
        "channels": channels,
        "instruments": mod["instruments"],
        "wavetables": mod["wavetables"],
    }


def build_instrument_timeline(meta, ch):
    timeline = []
    current = None
    for p_idx in ch["orderList"]:
        pat = ch["patterns"][p_idx] if p_idx < len(ch["patterns"]) else None
        rows = pat["rows"] if pat else []
        arr = []
        for r in range(meta["patternLength"]):
            raw = rows[r] if r < len(rows) else None
            if raw is not None:
                if "ins" in raw:
                    current = raw["ins"]
                if raw.get("note") == NOTE_OFF:
                    current = None
            arr.append(current)
        timeline.append(arr)
    return timeline


def midi_vlq(value):
    out = [value & 0x7F]
    value //= 128
    while value > 0:
        out.insert(0, (value & 0x7F) | 0x80)
        value //= 128
    return out


def midi_str(s):
    return [(ord(c) if 32 <= ord(c) <= 126 else 0x3F) for c in s]


def midi_uint32(v):
    return [(v >> 24) & 0xFF, (v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF]


def midi_uint16(v):
    return [(v >> 8) & 0xFF, v & 0xFF]


def midi_meta_event(event_type, data_bytes):
    return [0xFF, event_type] + midi_vlq(len(data_bytes)) + data_bytes


def midi_track_chunk(events):
    body = []
    for ev in events:
        body += midi_vlq(ev["deltaTicks"]) + ev["bytes"]
    body += midi_vlq(0) + [0xFF, 0x2F, 0x00]  # end of track
    return midi_str("MTrk") + midi_uint32(len(body)) + body


def our_note_to_midi_note(n):
    return n - (A_REF_NOTE - 69)


def midi_bend_value_for(semitone_offset):
    clamped = max(-MIDI_BEND_RANGE_SEMITONES, min(MIDI_BEND_RANGE_SEMITONES, semitone_offset))
    return max(0, min(16383, js_round(8192 + (clamped / MIDI_BEND_RANGE_SEMITONES) * 8191)))


def walk_channel_events(meta, ch):
    events = []
    sounding = None
    sounding_ins = None
    pitch_slide_rate = 0.0
    vibrato_speed = 0.0
    vibrato_depth = 0.0
    vibrato_phase = 0.0
    accum_pitch = 0.0
    last_bend_value = [8192]  # boxed so the nested helper can mutate it

    def emit_bend_reset(tick, ins_idx):
        if last_bend_value[0] != 8192:
            events.append({"tick": tick, "insIndex": ins_idx, "type": "bend", "bendValue": 8192})
            last_bend_value[0] = 8192

    midi_ticks_per_row = MIDI_TICKS_PER_AUDIO_TICK * meta["speeds"][0]

    for op in range(meta["orderLength"]):
        pat_idx = ch["orderList"][op]
        pat = ch["patterns"][pat_idx] if pat_idx < len(ch["patterns"]) else None
        for r in range(meta["patternLength"]):
            rows = pat["rows"] if pat else []
            raw = rows[r] if r < len(rows) else None
            row_tick = (op * meta["patternLength"] + r) * midi_ticks_per_row
            ins_idx = ch["insTimeline"][op][r]

            if raw is not None and "note" in raw:
                if sounding is not None:
                    events.append({"tick": row_tick, "insIndex": sounding_ins, "type": "off", "note": sounding})
                    sounding = None
                    sounding_ins = None
                pitch_slide_rate = 0.0
                vibrato_speed = 0.0
                vibrato_depth = 0.0
                vibrato_phase = 0.0
                accum_pitch = 0.0
                emit_bend_reset(row_tick, ins_idx)
                if raw["note"] != NOTE_OFF:
                    midi_note = our_note_to_midi_note(raw["note"])
                    events.append({"tick": row_tick, "insIndex": ins_idx, "type": "on", "note": midi_note})
                    sounding = midi_note
                    sounding_ins = ins_idx

            if raw is not None and raw.get("effects"):
                for e in raw["effects"]:
                    if not e:
                        continue
                    if e["code"] == 1:
                        pitch_slide_rate = e["value"] * PITCH_SLIDE_UNIT
                    elif e["code"] == 2:
                        pitch_slide_rate = -e["value"] * PITCH_SLIDE_UNIT
                    elif e["code"] == 4:
                        vibrato_speed = (e["value"] >> 4) * VIBRATO_SPEED_UNIT
                        vibrato_depth = (e["value"] & 0xF) * VIBRATO_DEPTH_UNIT

            if sounding is not None and (pitch_slide_rate != 0 or vibrato_depth != 0):
                for at in range(meta["speeds"][0]):
                    accum_pitch += pitch_slide_rate
                    v = midi_bend_value_for(accum_pitch + vibrato_depth * math.sin(2 * math.pi * vibrato_phase / 64))
                    if v != last_bend_value[0]:
                        events.append({
                            "tick": row_tick + at * MIDI_TICKS_PER_AUDIO_TICK,
                            "insIndex": ins_idx, "type": "bend", "bendValue": v,
                        })
                        last_bend_value[0] = v
                    vibrato_phase = (vibrato_phase + vibrato_speed) % 64

    end_tick = meta["orderLength"] * meta["patternLength"] * midi_ticks_per_row
    if sounding is not None:
        events.append({"tick": end_tick, "insIndex": sounding_ins, "type": "off", "note": sounding})
    emit_bend_reset(end_tick, sounding_ins)
    return events


def build_instrument_track_events(ins_index, all_events):
    evts = sorted((e for e in all_events if e["insIndex"] == ins_index), key=lambda e: e["tick"])
    midi_ch = ins_index % 16
    out = []
    last_tick = 0
    for e in evts:
        if e["type"] == "on":
            b = [0x90 | midi_ch, e["note"], 100]
        elif e["type"] == "off":
            b = [0x80 | midi_ch, e["note"], 0]
        else:
            v = e["bendValue"]
            b = [0xE0 | midi_ch, v & 0x7F, (v >> 7) & 0x7F]
        out.append({"deltaTicks": e["tick"] - last_tick, "bytes": b})
        last_tick = e["tick"]
    return out


def build_midi_bytes(data):
    song = build_song_model(read_fur_module(data))
    meta = song["meta"]
    for ch in song["channels"]:
        ch["insTimeline"] = build_instrument_timeline(meta, ch)

    row_duration_sec = meta["speeds"][0] / meta["tickRate"]
    beat_sec = (meta["highlights"][0] or 4) * row_duration_sec
    bpm = 60 / beat_sec
    us_per_quarter = js_round(60000000 / bpm)
    midi_ticks_per_row = MIDI_TICKS_PER_AUDIO_TICK * meta["speeds"][0]
    ppq = midi_ticks_per_row * (meta["highlights"][0] or 4)

    tempo_track = midi_track_chunk([
        {"deltaTicks": 0, "bytes": midi_meta_event(0x51, [(us_per_quarter >> 16) & 0xFF, (us_per_quarter >> 8) & 0xFF, us_per_quarter & 0xFF])},
        {"deltaTicks": 0, "bytes": midi_meta_event(0x03, midi_str((meta["name"] or "Lantern export") + " tempo"))},
    ])
    tracks = [tempo_track]

    all_events = []
    for ch in song["channels"]:
        all_events += walk_channel_events(meta, ch)

    for i, ins in enumerate(song["instruments"]):
        evts = build_instrument_track_events(i, all_events)
        if not evts:
            continue
        midi_ch = i % 16
        header = [
            {"deltaTicks": 0, "bytes": midi_meta_event(0x03, midi_str(ins.get("name") or ("Instrument " + str(i))))},
            {"deltaTicks": 0, "bytes": [0xB0 | midi_ch, 0x65, 0x00]},
            {"deltaTicks": 0, "bytes": [0xB0 | midi_ch, 0x64, 0x00]},
            {"deltaTicks": 0, "bytes": [0xB0 | midi_ch, 0x06, MIDI_BEND_RANGE_SEMITONES]},
            {"deltaTicks": 0, "bytes": [0xB0 | midi_ch, 0x64, 0x7F]},
            {"deltaTicks": 0, "bytes": [0xB0 | midi_ch, 0x65, 0x7F]},
        ]
        tracks.append(midi_track_chunk(header + evts))

    out = midi_str("MThd") + midi_uint32(6) + midi_uint16(1) + midi_uint16(len(tracks)) + midi_uint16(ppq)
    for tc in tracks:
        out += tc
    return bytes(out)


# --- Stage 3: convert stems, copy .fur, peak-normalize + copy .wav, write
# the .mid --------------------------------------------------------------

def wav_stream_params(path):
    result = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries", "stream=codec_name,sample_rate,channels",
         "-of", "json", path],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        fail("ERROR: ffprobe failed reading %s:\n%s" % (path, result.stderr))
    info = json.loads(result.stdout)["streams"][0]
    return info["codec_name"], info["sample_rate"], info["channels"]


def peak_normalize_wav(src, dst, target_db):
    # Two-pass: ffmpeg has no single-pass "normalize to peak X dB" filter,
    # so first measure the current true peak via volumedetect (a null-output
    # pass, nothing written), then apply the exact flat gain needed to land
    # it on target_db with a second real pass. Re-encodes with the same
    # codec/rate/channels as the source so this doesn't also silently
    # change the shipped file's format.
    detect = subprocess.run(
        ["ffmpeg", "-i", src, "-af", "volumedetect", "-f", "null", "-"],
        capture_output=True, text=True,
    )
    m = re.search(r"max_volume:\s*(-?\d+(?:\.\d+)?) dB", detect.stderr)
    if not m:
        fail("ERROR: could not read peak volume (volumedetect) for %s:\n%s" % (src, detect.stderr))
    current_peak_db = float(m.group(1))
    gain_db = target_db - current_peak_db

    codec, sample_rate, channels = wav_stream_params(src)
    result = subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-i", src,
         "-af", "volume=%.4fdB" % gain_db,
         "-c:a", codec, "-ar", str(sample_rate), "-ac", str(channels), dst],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        fail("ERROR: ffmpeg failed peak-normalizing %s:\n%s" % (src, result.stderr))
    print("peak-normalized %s -> %s (%.1fdB -> %.1fdB, gain %+.2fdB)" % (
        os.path.relpath(src, SHRINE_DIR), os.path.relpath(dst, SHRINE_DIR), current_peak_db, target_db, gain_db))


def convert_assets(trackname, data):
    os.makedirs(ASSETS_DIR, exist_ok=True)

    for i in range(4):
        src = os.path.join(INGEST_DIR, "%d.wav" % i)
        dst = os.path.join(ASSETS_DIR, "%d.ogg" % i)
        print("encoding %s -> %s ..." % (os.path.relpath(src, SHRINE_DIR), os.path.relpath(dst, SHRINE_DIR)))
        result = subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-i", src, "-c:a", "libvorbis", "-q:a", "3", dst],
            capture_output=True, text=True,
        )
        if result.returncode != 0:
            fail("ERROR: ffmpeg failed encoding %s:\n%s" % (src, result.stderr))

    fur_src = os.path.join(INGEST_DIR, trackname + ".fur")
    fur_dst = os.path.join(ASSETS_DIR, trackname + ".fur")
    shutil.copyfile(fur_src, fur_dst)
    print("copied %s -> %s" % (os.path.relpath(fur_src, SHRINE_DIR), os.path.relpath(fur_dst, SHRINE_DIR)))

    wav_src = os.path.join(INGEST_DIR, trackname + ".wav")
    wav_dst = os.path.join(ASSETS_DIR, trackname + ".wav")
    peak_normalize_wav(wav_src, wav_dst, WAV_PEAK_NORMALIZE_DB)

    mid_path = os.path.join(ASSETS_DIR, trackname + ".mid")
    with open(mid_path, "wb") as f:
        f.write(build_midi_bytes(data))
    print("wrote %s" % os.path.relpath(mid_path, SHRINE_DIR))
    print()


# --- Stage 4: regenerate the HTML ------------------------------------------

DOWNLOAD_BUTTONS_JS = """
  // Static-asset downloads (the source .fur module, the full mixed .wav,
  // and the MIDI export) — same temporary-<a> pattern as
  // exportCoverArtPng() above, but pointing at a real file in ASSETS/
  // instead of a Blob. MIDI generation itself now happens once at build
  // time (build.py's build_midi_bytes(), a port of the buildMidiBytes()
  // this used to run in-browser) rather than on every click. Uses the
  // build's trackname (matches the actual ASSETS/ filenames on disk), not
  // song.meta.name — that's the human song title from the Furnace
  // project, which can differ from the filename and would 404.
  function downloadAsset(filename){
    var a = document.createElement('a');
    a.href = 'ASSETS/' + filename;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  }
  document.getElementById('downloadFurBtn').addEventListener('click', function(){
    downloadAsset('%s.fur');
  });
  document.getElementById('downloadWavBtn').addEventListener('click', function(){
    downloadAsset('%s.wav');
  });
  document.getElementById('exportMidiBtn').addEventListener('click', function(){
    downloadAsset('%s.mid');
  });
"""


def find_template(shrine_name, out_path):
    """Prefer the exact trackname-matched output if it's already there
    (repeat build of the same track). Otherwise this shrine slot is being
    swapped to a different track — fall back to whatever other shrine HTML
    already exists here and use it as the structural template, since the
    JS/CSS shell is identical across tracks."""
    if os.path.isfile(out_path):
        return out_path
    candidates = [
        f for f in os.listdir(SHRINE_DIR)
        if f.startswith(shrine_name + "-") and f.endswith(".html") and not f.endswith("-backup.html")
    ]
    if not candidates:
        fail("ERROR: no existing shrine HTML found at %s (or anywhere else in "
             "%s) to use as a template — this script updates an existing "
             "shrine HTML in place, it doesn't create one from scratch." % (out_path, SHRINE_DIR))
    if len(candidates) > 1:
        candidates.sort(key=lambda f: os.path.getmtime(os.path.join(SHRINE_DIR, f)), reverse=True)
        print("NOTE: multiple candidate templates found (%s) — using the most "
              "recently modified, %r." % (", ".join(candidates), candidates[0]))
    return os.path.join(SHRINE_DIR, candidates[0])


def regenerate_html(trackname, data):
    shrine_name = os.path.basename(SHRINE_DIR)
    out_name = "%s-%s.html" % (shrine_name, trackname)
    out_path = os.path.join(SHRINE_DIR, out_name)

    template_path = find_template(shrine_name, out_path)
    track_changed = template_path != out_path

    with open(template_path, "r", encoding="utf-8") as f:
        html = f.read()

    if 'id="fur-json"' not in html or "STEM_FILES" not in html:
        fail("ERROR: %s doesn't look like a recognized shrine template "
             "(missing the fur-json script tag or STEM_FILES array) — "
             "refusing to overwrite it." % template_path)

    backup_path = os.path.splitext(template_path)[0] + "-backup.html"
    shutil.copyfile(template_path, backup_path)
    print("backed up previous HTML -> %s" % os.path.relpath(backup_path, SHRINE_DIR))

    if track_changed:
        os.remove(template_path)
        print("removed stale %s (superseded by %s; ASSETS/ is shared and now "
              "holds %s's data, so the old file would no longer be self-"
              "consistent — it's preserved in the backup above)"
              % (os.path.relpath(template_path, SHRINE_DIR), out_name, trackname))

    # 1. minify the embedded JSON into a single line
    minified = json.dumps(data, separators=(",", ":"))
    html, n = re.subn(
        r'(<script id="fur-json" type="application/json">\n?).*?(\n?</script>)',
        lambda m: m.group(1) + minified + m.group(2),
        html,
        count=1,
        flags=re.DOTALL,
    )
    if n != 1:
        fail("ERROR: could not find the fur-json <script> block to replace.")

    # 2. repoint the stem paths at ASSETS/N.ogg, leaving trailing comments
    # alone. Idempotent: matches either the original 'stems/name-N.ogg' form
    # or an already-migrated 'ASSETS/N.ogg' form, since re-running this on a
    # previously-built file (same track, or reused as a fallback template
    # for a different one) is a normal case, not just a first-ever build.
    def replace_stem(m):
        return "'ASSETS/%s.ogg'" % m.group(1)
    html, n = re.subn(r"'(?:stems/[^']*?-|ASSETS/)(\d)\.ogg'", replace_stem, html)
    if n != 4:
        fail("ERROR: expected to replace 4 stem paths in STEM_FILES, replaced %d." % n)

    # 3. add the two download buttons around exportMidiBtn \u2014 strip any
    # copies left by a previous run first, so re-running this script never
    # duplicates them.
    html = re.sub(r'\s*<button id="downloadFurBtn"[^>]*>[^<]*</button>', '', html)
    html = re.sub(r'\s*<button id="downloadWavBtn"[^>]*>[^<]*</button>', '', html)
    fur_btn = ('<button id="downloadFurBtn" type="button" '
               'title="download the original Furnace module (.fur)">\u21e9 .fur</button>')
    wav_btn = ('<button id="downloadWavBtn" type="button" '
               'title="download the full mixed song as .wav">\u21e9 WAV</button>')
    html, n = re.subn(
        r'(\s*)(<button id="exportMidiBtn"[^>]*>[^<]*</button>)',
        lambda m: m.group(1) + fur_btn + m.group(1) + m.group(2) + m.group(1) + wav_btn,
        html,
        count=1,
    )
    if n != 1:
        fail("ERROR: could not find exportMidiBtn to insert the new download buttons next to.")

    # 4a. remove the old in-browser MIDI generator (buildMidiBytes() and
    # everything it depended on) — MIDI generation now happens once at
    # build time (build_midi_bytes() above) instead. Idempotent: on a file
    # that's already been through this, the block is simply gone already
    # and this is a no-op (n==0).
    html, n = re.subn(
        r"\n\n  // ============================================================\n"
        r"  // \"Export MIDI\".*?"
        r"document\.getElementById\('exportMidiBtn'\)\.addEventListener\('click', exportMidi\);\n",
        "\n",
        html,
        flags=re.DOTALL,
    )
    if n not in (0, 1):
        fail("ERROR: expected at most one old Export-MIDI JS block, found %d." % n)
    html = re.sub(r"\n\s*buildMidiBytes: buildMidiBytes,", "", html)

    # 4b. wire up the three download buttons, right after exportCoverArtPng's
    # own wiring — strip any previous copy first for idempotency. Matches
    # through downloadWavBtn's closing brace, then optionally consumes a
    # trailing exportMidiBtn block too if present — a template built by an
    # older version of this script (before the exportMidiBtn wiring existed
    # here) only has the first two, so the optional tail must not be
    # required or that older form is left behind, duplicating everything
    # inserted below.
    html = re.sub(
        r"\n  // Static-asset downloads.*?downloadWavBtn.*?\n  \}\);\n"
        r"(?:  document\.getElementById\('exportMidiBtn'\)\.addEventListener\('click', function\(\)\{\n.*?\n  \}\);\n)?",
        "\n",
        html,
        flags=re.DOTALL,
    )
    js_block = DOWNLOAD_BUTTONS_JS % (trackname, trackname, trackname)
    html, n = re.subn(
        r"(document\.getElementById\('coverArt'\)\.addEventListener\('click', exportCoverArtPng\);\n)",
        lambda m: m.group(1) + js_block,
        html,
        count=1,
    )
    if n != 1:
        fail("ERROR: could not find exportCoverArtPng's click listener to attach the new handlers after.")

    # 5. song title (the h1's bold track name), subtitle (with today's build
    # date), and both footer lines. All keyed off stable tag/class anchors
    # rather than the previous text, so re-running this is idempotent no
    # matter what a prior run (for this track or another) left behind.
    song_title = html_escape(data.get("songInfo", {}).get("name") or trackname)
    html, n = re.subn(
        r'(<h1>[^<]*<b>)[^<]*(</b></h1>)',
        lambda m: m.group(1) + song_title + m.group(2),
        html,
        count=1,
    )
    if n != 1:
        fail("ERROR: could not find the <h1><b>...</b></h1> track name to update.")

    build_date = datetime.date.today().strftime("%y%m%d")
    subtitle_html = (
        'Lantern Music Player - build %s, track composed in '
        '<a href="%s" target="_blank" rel="noreferrer noopener">Furnace</a>.'
        % (build_date, FURNACE_URL)
    )
    html, n = re.subn(
        r'<div class="subtitle">.*?</div>',
        lambda m: '<div class="subtitle">' + subtitle_html + '</div>',
        html,
        count=1,
        flags=re.DOTALL,
    )
    if n != 1:
        fail("ERROR: could not find the .subtitle div to update.")

    footer_html = (
        '<footer>\n'
        '    <span>Music License: CC0 / Public Domain  ·  Code License: MIT · '
        '<a href="%s" target="_blank" rel="noreferrer noopener">view source</a></span>\n'
        '    <span><a href="%s" target="_blank" rel="noreferrer noopener">johnoestmannmusic.com</a></span>\n'
        '  </footer>'
        % (GITHUB_REPO_URL, COMPOSER_SITE_URL)
    )
    html, n = re.subn(
        r'<footer>.*?</footer>',
        lambda m: footer_html,
        html,
        count=1,
        flags=re.DOTALL,
    )
    if n != 1:
        fail("ERROR: could not find the <footer> block to update.")

    with open(out_path, "w", encoding="utf-8") as f:
        f.write(html)
    print("wrote %s" % os.path.relpath(out_path, SHRINE_DIR))
    return out_path


def main():
    trackname = find_trackname()
    check_required_files(trackname)

    json_path = os.path.join(INGEST_DIR, trackname + ".json")
    txt_path = os.path.join(INGEST_DIR, trackname + ".txt")
    with open(json_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    sanity_check(data, txt_path)
    convert_assets(trackname, data)
    out_path = regenerate_html(trackname, data)

    print("\nBuild complete:")
    print("  %s" % out_path)
    print("  %s/ (0.ogg..3.ogg, %s.fur, %s.wav, %s.mid)" % (ASSETS_DIR, trackname, trackname, trackname))


if __name__ == "__main__":
    main()
