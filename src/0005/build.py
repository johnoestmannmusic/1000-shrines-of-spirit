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
  ASSETS/0.ogg .. 3.ogg, ASSETS/<trackname>.fur, ASSETS/<trackname>.wav
  <shrine-folder>-<trackname>.html   (e.g. 0005-flight_school.html)
  <shrine-folder>-<trackname>-backup.html  (previous version, if any)
"""
import json
import os
import re
import shutil
import subprocess
import sys

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


# --- Stage 3: convert stems, copy .fur/.wav --------------------------------

def convert_assets(trackname):
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

    for ext in (".fur", ".wav"):
        src = os.path.join(INGEST_DIR, trackname + ext)
        dst = os.path.join(ASSETS_DIR, trackname + ext)
        shutil.copyfile(src, dst)
        print("copied %s -> %s" % (os.path.relpath(src, SHRINE_DIR), os.path.relpath(dst, SHRINE_DIR)))
    print()


# --- Stage 4: regenerate the HTML ------------------------------------------

DOWNLOAD_BUTTONS_JS = """
  // Static-asset downloads (the source .fur module and the full mixed
  // .wav) — same temporary-<a> pattern as exportMidi()/exportCoverArtPng()
  // above, but pointing at a real file in ASSETS/ instead of a Blob.
  function downloadAsset(filename){
    var a = document.createElement('a');
    a.href = 'ASSETS/' + filename;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  }
  document.getElementById('downloadFurBtn').addEventListener('click', function(){
    downloadAsset((song.meta.name || '%s') + '.fur');
  });
  document.getElementById('downloadWavBtn').addEventListener('click', function(){
    downloadAsset((song.meta.name || '%s') + '.wav');
  });
"""


def regenerate_html(trackname, data):
    shrine_name = os.path.basename(SHRINE_DIR)
    out_name = "%s-%s.html" % (shrine_name, trackname)
    out_path = os.path.join(SHRINE_DIR, out_name)

    if not os.path.isfile(out_path):
        fail("ERROR: expected an existing template at %s to regenerate — "
             "this script updates an existing shrine HTML in place, it "
             "doesn't create one from scratch." % out_path)

    with open(out_path, "r", encoding="utf-8") as f:
        html = f.read()

    if 'id="fur-json"' not in html or "STEM_FILES" not in html:
        fail("ERROR: %s doesn't look like a recognized shrine template "
             "(missing the fur-json script tag or STEM_FILES array) — "
             "refusing to overwrite it." % out_path)

    backup_path = os.path.join(SHRINE_DIR, "%s-%s-backup.html" % (shrine_name, trackname))
    shutil.copyfile(out_path, backup_path)
    print("backed up previous HTML -> %s" % os.path.relpath(backup_path, SHRINE_DIR))

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

    # 2. repoint the stem paths at ASSETS/N.ogg, leaving trailing comments alone
    def replace_stem(m):
        return "'ASSETS/%s.ogg'" % m.group(1)
    html, n = re.subn(r"'stems/[^']*?-(\d)\.ogg'", replace_stem, html)
    if n != 4:
        fail("ERROR: expected to replace 4 stem paths in STEM_FILES, replaced %d." % n)

    # 3. add the two download buttons around exportMidiBtn
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

    # 4. wire up the two new buttons, right after exportMidi's own wiring
    js_block = DOWNLOAD_BUTTONS_JS % (trackname, trackname)
    html, n = re.subn(
        r"(document\.getElementById\('exportMidiBtn'\)\.addEventListener\('click', exportMidi\);\n)",
        lambda m: m.group(1) + js_block,
        html,
        count=1,
    )
    if n != 1:
        fail("ERROR: could not find exportMidiBtn's click listener to attach the new handlers after.")

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
    convert_assets(trackname)
    out_path = regenerate_html(trackname, data)

    print("\nBuild complete:")
    print("  %s" % out_path)
    print("  %s/ (0.ogg..3.ogg, %s.fur, %s.wav)" % (ASSETS_DIR, trackname, trackname))


if __name__ == "__main__":
    main()
