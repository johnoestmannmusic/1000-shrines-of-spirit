mod render_worker;

use arc_swap::{ArcSwap, ArcSwapOption};
use prism_dsp::render::{render_frozen_loop, LoopBufferData, DEFAULT_ROOT_NOTE};
use prism_dsp::resample::resample_linear;
use prism_dsp::voice::{AdsrSettings, VoiceManager};
use nih_plug::prelude::*;
use nih_plug_egui::resizable_window::ResizableWindow;
use nih_plug_egui::{create_egui_editor, egui, widgets, EguiState};
use render_worker::{RenderRequest, RenderTrigger, RenderWorker};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

/// Sentinel for "no note has been received yet" in `last_note`, distinct from
/// any real MIDI note number (0-127).
const NO_NOTE: u8 = 255;

/// YYYYMMDD, stamped at compile time by `build.rs` - shown in the editor's
/// bottom-right corner and saved into every exported preset, so it's
/// possible to tell which plugin version made a given sound.
const BUILD_NUMBER: &str = env!("SPECTRALPRISM_BUILD_NUMBER");

/// The loop buffer is baked from a source sample (the built-in placeholder
/// tone until a real file is loaded via the editor's "Load Sample" button)
/// and played polyphonically through `VoiceManager`, driven by real MIDI
/// note on/off. Freeze Point / Formant Shift / Stereo Width are real
/// automatable params, but re-rendering the frozen loop on every change is
/// too expensive for the audio thread - `process()` only *notices* a param
/// change and hands it to a background `RenderWorker`, which publishes the
/// new loop into `loop_buffer` (an `ArcSwap` so the audio thread can read
/// the latest version lock-free without ever blocking on the render).
pub struct PrismPlugin {
    params: Arc<PrismPluginParams>,
    /// The audio being frozen. An `ArcSwap` (not a plain `Arc`) so the
    /// editor's "Load Sample" button can swap in newly loaded audio without
    /// touching the audio thread - `RenderWorker` always reads whatever is
    /// current at the moment it renders.
    source: Arc<ArcSwap<Vec<Vec<f32>>>>,
    /// The real file's name once the user has loaded one, `None` while
    /// still on the built-in placeholder tone. Kept on the plugin (not in
    /// `PrismEditorState`) specifically so it survives the editor window
    /// being closed and reopened within the same plugin instance -
    /// `PrismEditorState` is recreated fresh every time `editor()` is
    /// called, but `source` itself (and therefore what's actually loaded)
    /// is not. Also doubles as the "[ Load Sample ]" sign's gate in
    /// `draw_freeze_point_waveform`.
    loaded_filename: Arc<ArcSwapOption<String>>,
    loop_buffer: Arc<ArcSwap<LoopBufferData>>,
    /// Exists from construction, independent of `worker` (which is only
    /// spawned once `initialize()` runs) - see `RenderWorker::spawn`'s doc
    /// comment for why that independence matters. The editor captures a
    /// clone of *this*, not something derived from `worker`, so it can
    /// trigger a render even if it was created before `initialize()` ran.
    trigger: RenderTrigger,
    worker: Option<RenderWorker>,
    voices: VoiceManager,
    sample_rate: f32,
    /// Raw MIDI pitch wheel position, normalized `[0, 1]` with `0.5` = no
    /// bend (nih-plug's own convention for `NoteEvent::MidiPitchBend`, see
    /// `process()`). Audio-thread-only - not a param, since the wheel's
    /// live position isn't a settable value, only how far it's *allowed* to
    /// bend is (`PrismPluginParams::pitch_bend_range_semitones`).
    pitch_bend_normalized: f32,
    /// The params a render has already been requested for (or that were
    /// used for the initial synchronous render). Compared against the
    /// current param values each block to detect a change at all.
    last_requested: RenderRequest,
    /// A detected change waiting for the throttle window to open. Always
    /// overwritten with the latest observed value (never queued), so once
    /// the throttle allows a send, it's always the most current params -
    /// never a stale intermediate value from partway through a drag.
    pending_request: Option<RenderRequest>,
    /// Counts down in samples; a new render is only actually requested once
    /// this reaches zero. See `RENDER_THROTTLE_MS` for why this exists.
    throttle_countdown: i64,
    /// Last MIDI note number received (`NO_NOTE` until the first one
    /// arrives), and the voice count read after this block's
    /// `VoiceManager::process_block` call - so a voice that just finished
    /// this block is already reflected. Shared with the editor via `Arc` so
    /// the GUI can poll them each frame without touching the audio thread,
    /// matching the pattern nih-plug's own `gain_gui_egui` example uses for
    /// its peak meter. Purely a diagnostic display (see FREEZE-BUG-001's
    /// false-alarm follow-up) - not read anywhere in the DSP path itself.
    last_note: Arc<AtomicU8>,
    active_voice_count: Arc<AtomicU8>,
}

/// Minimum spacing between actual render requests, regardless of how often
/// the params themselves change.
///
/// Without this, a host automating (or a human dragging) Freeze Point
/// smoothly triggers a fresh render on nearly every processed block - each
/// landing mid-crossfade from the *previous* one, so the crossfades never
/// get to finish and instead pile up into a continuous warble. This isn't a
/// bug in the crossfade itself: two different freeze-point positions are
/// genuinely different, unrelated spectral snapshots (not points on a smooth
/// continuum), so some character change when scrubbing is inherent to this
/// DSP approach. But letting each committed change's `BUFFER_CROSSFADE_MS`
/// window actually complete before the next one starts turns "constant
/// stutter" into "occasional smooth morph" - throttling to roughly
/// `BUFFER_CROSSFADE_MS` or a bit more keeps them from overlapping.
const RENDER_THROTTLE_MS: f32 = 100.0;

/// The editor's base (1x scale, minimum-drag-size) window size in logical
/// points - the size its two-column layout is designed to exactly fill.
/// Also the reference size `apply_gui_scale` divides the *actual* window
/// size by to compute the current scale factor. The window actually opens
/// at `DEFAULT_SCALE` times this (see `EguiState::from_size` below), not at
/// this size directly - this is just the floor `ResizableWindow`'s
/// `min_size` won't let it shrink past.
/// Widened and shortened from an earlier (620x560) guess, which was too
/// narrow for the two-column layout at scale (elements got cut off
/// horizontally) while leaving too much unused vertical space. Height
/// bumped again (460->600) after Pitch Bend Range/Pan Center/Pan Width
/// (`FREEZE-PLAN-019`/`FREEZE-PLAN-020`) added three more rows to the right
/// column, then once more (600->640) after the Import/Export Preset row
/// (`FREEZE-PLAN-022`) added one more row above the two-column section -
/// each time forcing a scroll to see the new controls at the default size
/// until bumped. The `ScrollArea` safety net (see `editor()`) means nothing
/// is ever actually clipped/lost in the meantime, just not visible without
/// scrolling.
const BASE_EDITOR_WIDTH: u32 = 800;
const BASE_EDITOR_HEIGHT: u32 = 640;
/// The editor opens at this multiple of the base size by default (matching
/// `apply_gui_scale`'s scale factor, since the two are computed from the
/// same base) - requested directly ("too small to read" at 1x, then a
/// further +25% on top of the first bump). Still freely resizable larger
/// (or back down to 1x) afterward via the corner.
const DEFAULT_SCALE: f32 = 1.875;

/// Shared by `draw_freeze_point_waveform` and `draw_adsr_graph` so the two
/// side-by-side graph boxes always line up at the same height, regardless
/// of scale - requested directly after the ADSR graph's own (taller) 90.0
/// default made the two columns visibly mismatched.
const GRAPH_HEIGHT: f32 = 70.0;

/// Blank space around the outside of the whole editor's content, in 1x-scale
/// points (multiplied by `scale` like everything else) - requested directly
/// in Reaper, where content was otherwise touching the window edges.
/// `16.0 * DEFAULT_SCALE` (1.875) is exactly 30px, matching what was asked
/// for at the default scale.
const EDITOR_MARGIN: f32 = 16.0;

/// Light palette lifted from `src/0006/index.html`'s light-mode `:root`
/// overrides (`--bg`/`--panel`/`--edge`/`--edge-hover`/`--ink`/`--dim`/
/// `--surface-deep`) - this plugin's own visual reference, see
/// `FREEZE-PLAN-011`/`FREEZE-PLAN-013`. `COLOR_BG`/`COLOR_PANEL` are pure
/// white rather than 0006's own slightly-off-white `--bg` (`#fafafa`), per
/// explicit request. The accent is `COLOR_ACCENT`, not 0006's light-mode
/// `--amber` - see that constant's own doc comment.
const COLOR_BG: egui::Color32 = egui::Color32::from_rgb(0xff, 0xff, 0xff);
const COLOR_PANEL: egui::Color32 = egui::Color32::from_rgb(0xff, 0xff, 0xff);
const COLOR_EDGE: egui::Color32 = egui::Color32::from_rgb(0xdc, 0xdd, 0xe0);
const COLOR_EDGE_HOVER: egui::Color32 = egui::Color32::from_rgb(0xb3, 0xb8, 0xbd);
const COLOR_INK: egui::Color32 = egui::Color32::from_rgb(0x2e, 0x34, 0x40);
const COLOR_DIM: egui::Color32 = egui::Color32::from_rgb(0x5e, 0x64, 0x70);
/// Not 0006's own light-theme accent (`#3f9e0e`) - the specific green the
/// user asked for, which 0006 actually uses elsewhere as a hardcoded
/// "on/active" highlight (`.spectral-btn.fusion-on`, the mode-switch toggle).
const COLOR_ACCENT: egui::Color32 = egui::Color32::from_rgb(0x6c, 0xd7, 0x3c);
const COLOR_SURFACE_DEEP: egui::Color32 = egui::Color32::from_rgb(0xee, 0xf0, 0xe9);
const COLOR_ERROR: egui::Color32 = egui::Color32::from_rgb(0xc0, 0x36, 0x2f);

/// Bundled locally (copied from `src/0006/ASSETS/`) rather than referenced
/// across projects by relative path, so this plugin doesn't depend on the
/// rest of the repo layout to build correctly.
const MEDODICA_FONT_BYTES: &[u8] = include_bytes!("../assets/MedodicaRegular.otf");
const MEDODICA_FONT_NAME: &str = "Medodica";

/// Installs the bundled Medodica font (see `MEDODICA_FONT_BYTES`) as the
/// first choice for both the proportional and monospace font families -
/// 0006 uses it as its one font for everything (`--mono: 'Medodica', ...
/// monospace`), so every `egui::TextStyle` (which all default to
/// `FontFamily::Proportional` except `Monospace` itself) should pick it up
/// too. Called once, from the editor's `build` callback (not per-frame -
/// fonts don't need to be reloaded every pass).
fn install_medodica_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        MEDODICA_FONT_NAME.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(MEDODICA_FONT_BYTES)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().insert(0, MEDODICA_FONT_NAME.to_owned());
    }
    ctx.set_fonts(fonts);
}

/// Recolors every default `egui::Visuals` field this editor actually uses
/// to 0006's light palette (see the `COLOR_*` constants above), so buttons,
/// sliders, panels, and text match that project's look instead of egui's
/// stock dark theme. Called once, from the editor's `build` callback - the
/// palette itself never changes at runtime, only `apply_gui_scale` (sizes)
/// needs to run every frame.
fn apply_theme(ctx: &egui::Context) {
    install_medodica_font(ctx);
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        v.dark_mode = false;
        v.window_fill = COLOR_BG;
        v.panel_fill = COLOR_BG;
        v.faint_bg_color = COLOR_PANEL;
        v.extreme_bg_color = COLOR_SURFACE_DEEP;
        v.code_bg_color = COLOR_SURFACE_DEEP;
        v.error_fg_color = COLOR_ERROR;
        v.warn_fg_color = COLOR_ACCENT;
        v.hyperlink_color = COLOR_ACCENT;
        v.selection.bg_fill = COLOR_ACCENT;
        v.selection.stroke = egui::Stroke::new(1.0, COLOR_ACCENT);

        v.widgets.noninteractive.bg_fill = COLOR_PANEL;
        v.widgets.noninteractive.weak_bg_fill = COLOR_PANEL;
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, COLOR_EDGE);
        v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, COLOR_INK);

        v.widgets.inactive.bg_fill = COLOR_SURFACE_DEEP;
        v.widgets.inactive.weak_bg_fill = COLOR_SURFACE_DEEP;
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, COLOR_EDGE);
        v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, COLOR_INK);

        v.widgets.hovered.bg_fill = COLOR_EDGE;
        v.widgets.hovered.weak_bg_fill = COLOR_EDGE;
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, COLOR_EDGE_HOVER);
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, COLOR_INK);

        v.widgets.active.bg_fill = COLOR_ACCENT;
        v.widgets.active.weak_bg_fill = COLOR_ACCENT;
        v.widgets.active.bg_stroke = egui::Stroke::new(1.0, COLOR_ACCENT);
        // Dark ink on the light-green accent fill reads better than white -
        // the accent is bright enough that white text washes out on it.
        v.widgets.active.fg_stroke = egui::Stroke::new(1.0, COLOR_INK);

        v.widgets.open.bg_fill = COLOR_SURFACE_DEEP;
        v.widgets.open.weak_bg_fill = COLOR_SURFACE_DEEP;
        v.widgets.open.bg_stroke = egui::Stroke::new(1.0, COLOR_ACCENT);
        v.widgets.open.fg_stroke = egui::Stroke::new(1.0, COLOR_INK);
    });
}

#[derive(Params)]
struct PrismPluginParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// The loaded sample's file path, if any - not a `FloatParam`, but
    /// persisted the same way `editor_state` is (nih-plug's own state
    /// save/restore, backing both a host's project recall and preset
    /// files - see `FREEZE-PLAN-010`). Without this, reopening a saved
    /// project or preset would restore every param correctly but silently
    /// revert to the built-in placeholder tone. Re-decoded from disk in
    /// `PrismPlugin::initialize()`/`apply_preset()` when present - if the
    /// file has since moved or been deleted, that just falls back to
    /// whatever's already loaded rather than failing outright.
    #[persist = "sample-path"]
    sample_path: Mutex<Option<PathBuf>>,

    /// Name of the last preset loaded or saved, if any - persisted the same
    /// way as `sample_path` above, for the same reason: without this, a
    /// saved host project restores every param correctly (`FloatParam`s
    /// already persist on their own) but the editor's preset browser shows
    /// "(no preset)" after reopening, even though the actual sound came
    /// back fine. Purely a GUI display/navigation aid (drives the combo
    /// box's selection and Prev/Next's starting point) - never read by the
    /// DSP path.
    #[persist = "selected-preset"]
    selected_preset: Mutex<Option<String>>,

    #[id = "freeze_point"]
    pub freeze_point: FloatParam,

    #[id = "formant_shift"]
    pub formant_shift: FloatParam,

    #[id = "stereo_width"]
    pub stereo_width: FloatParam,

    /// Length of the frozen loop buffer itself (not the FreezeFft/playback
    /// duration) - shorter values reduce both the in-memory buffer size and
    /// the exported WAV's file size. Purely a size/perceptual tradeoff, not
    /// a technical one: any hop-aligned length loops with the same
    /// continuity (see `render::render_frozen_loop`'s doc comment).
    #[id = "loop_length"]
    pub loop_length_seconds: FloatParam,

    #[id = "attack"]
    pub attack: FloatParam,

    #[id = "decay"]
    pub decay: FloatParam,

    #[id = "sustain"]
    pub sustain: FloatParam,

    #[id = "release"]
    pub release: FloatParam,

    #[id = "velocity_sensitivity"]
    pub velocity_sensitivity: FloatParam,

    /// Maximum semitones the pitch wheel can bend by (at full deflection
    /// either direction) - the wheel's own raw MIDI position isn't a param
    /// at all (it's a continuous performance control, not a settable
    /// value), only how far it's *allowed* to bend is.
    #[id = "pitch_bend_range"]
    pub pitch_bend_range_semitones: FloatParam,

    #[id = "pan_center"]
    pub pan_center_pct: FloatParam,

    #[id = "pan_width"]
    pub pan_width_pct: FloatParam,
}

fn silent_loop_buffer() -> LoopBufferData {
    LoopBufferData { channels: Vec::new(), sample_rate: 1.0, root_note: DEFAULT_ROOT_NOTE }
}

impl Default for PrismPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(PrismPluginParams::default()),
            source: Arc::new(ArcSwap::new(Arc::new(Vec::new()))),
            loaded_filename: Arc::new(ArcSwapOption::from(None)),
            loop_buffer: Arc::new(ArcSwap::new(Arc::new(silent_loop_buffer()))),
            trigger: RenderTrigger::new(),
            worker: None,
            voices: VoiceManager::new(1.0, DEFAULT_ROOT_NOTE),
            sample_rate: 1.0,
            pitch_bend_normalized: 0.5,
            last_requested: RenderRequest {
                freeze_point_pct: 50.0,
                formant_shift_semitones: 0.0,
                stereo_width_pct: 30.0,
                loop_length_seconds: prism_dsp::render::DEFAULT_LOOP_SECONDS,
            },
            pending_request: None,
            throttle_countdown: 0,
            last_note: Arc::new(AtomicU8::new(NO_NOTE)),
            active_voice_count: Arc::new(AtomicU8::new(0)),
        }
    }
}

impl Default for PrismPluginParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(
                (BASE_EDITOR_WIDTH as f32 * DEFAULT_SCALE) as u32,
                (BASE_EDITOR_HEIGHT as f32 * DEFAULT_SCALE) as u32,
            ),
            sample_path: Mutex::new(None),
            selected_preset: Mutex::new(None),
            freeze_point: FloatParam::new("Freeze Point", 50.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %"),
            formant_shift: FloatParam::new(
                "Formant Shift",
                0.0,
                FloatRange::Linear { min: -12.0, max: 12.0 },
            )
            .with_unit(" st"),
            stereo_width: FloatParam::new("Stereo Width", 30.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %"),
            loop_length_seconds: FloatParam::new(
                "Loop Length",
                prism_dsp::render::DEFAULT_LOOP_SECONDS,
                FloatRange::Linear {
                    min: prism_dsp::render::MIN_LOOP_SECONDS,
                    max: prism_dsp::render::MAX_LOOP_SECONDS,
                },
            )
            .with_unit(" s"),
            attack: FloatParam::new(
                "Attack",
                prism_dsp::voice::ATTACK_MS,
                FloatRange::Skewed { min: 1.0, max: 2000.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_unit(" ms"),
            decay: FloatParam::new(
                "Decay",
                prism_dsp::voice::DECAY_MS,
                FloatRange::Skewed { min: 1.0, max: 2000.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_unit(" ms"),
            sustain: FloatParam::new(
                "Sustain",
                prism_dsp::voice::SUSTAIN_LEVEL * 100.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_unit(" %"),
            release: FloatParam::new(
                "Release",
                prism_dsp::voice::RELEASE_MS,
                FloatRange::Skewed { min: 1.0, max: 5000.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_unit(" ms"),
            velocity_sensitivity: FloatParam::new(
                "Velocity Sensitivity",
                prism_dsp::voice::DEFAULT_VELOCITY_SENSITIVITY * 100.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_unit(" %"),
            pitch_bend_range_semitones: FloatParam::new(
                "Pitch Bend Range",
                2.0, // the MIDI/GM standard default of +/-2 semitones
                FloatRange::Linear { min: 0.0, max: 24.0 },
            )
            .with_unit(" st"),
            pan_center_pct: FloatParam::new("Pan Center", 0.0, FloatRange::Linear { min: -100.0, max: 100.0 })
                .with_unit(" %"),
            pan_width_pct: FloatParam::new("Random Pan Width", 0.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %"),
        }
    }
}

/// A short synthetic tone to freeze - the built-in placeholder until a real
/// file is loaded via the editor. Brightness (the balance between the
/// fundamental and its upper harmonics) sweeps over the tone's duration,
/// specifically so that different Freeze Point values capture genuinely
/// different-sounding moments - a *constant* tone would give every Freeze
/// Point nearly identical magnitude content differing only in essentially
/// arbitrary starting phase, which is a poor demonstration of what Freeze
/// Point is for.
fn synthetic_source(sample_rate: f32, seconds: f32) -> Vec<f32> {
    let len = (sample_rate * seconds) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let brightness = i as f32 / len as f32; // 0.0 at start, 1.0 at end
            0.5 * (t * 220.0 * std::f32::consts::TAU).sin()
                + (0.05 + 0.5 * brightness) * (t * 440.0 * std::f32::consts::TAU).sin()
                + (0.5 * brightness) * (t * 660.0 * std::f32::consts::TAU).sin()
        })
        .collect()
}

/// Decodes a WAV file into one `Vec<f32>` per channel (interleaved samples
/// deinterleaved), normalizing integer formats to `[-1.0, 1.0]`. Returns the
/// file's own sample rate alongside - the caller is responsible for
/// resampling to the plugin's operating rate if they differ.
fn load_wav_channels(path: &Path) -> Result<(Vec<Vec<f32>>, f32), String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("couldn't open WAV: {e}"))?;
    let spec = reader.spec();
    let num_channels = (spec.channels as usize).max(1);
    let sample_rate = spec.sample_rate as f32;

    let mut channels: Vec<Vec<f32>> = vec![Vec::new(); num_channels];
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (i, sample) in reader.samples::<f32>().enumerate() {
                let s = sample.map_err(|e| format!("error reading sample: {e}"))?;
                channels[i % num_channels].push(s);
            }
        }
        hound::SampleFormat::Int => {
            let max_amplitude = (1i64 << (spec.bits_per_sample - 1)) as f32;
            for (i, sample) in reader.samples::<i32>().enumerate() {
                let s = sample.map_err(|e| format!("error reading sample: {e}"))? as f32 / max_amplitude;
                channels[i % num_channels].push(s);
            }
        }
    }

    if channels.iter().all(|c| c.is_empty()) {
        return Err("WAV file contains no audio samples".to_string());
    }

    Ok((channels, sample_rate))
}

/// Brings a loaded file's audio to the plugin's current operating rate -
/// without this, a file whose native rate differs from the host's would
/// play back pitch/speed-shifted, since `VoiceManager` reads the frozen
/// loop assuming it's already at the plugin's operating rate.
fn prepare_source_for_plugin_rate(channels: Vec<Vec<f32>>, file_rate: f32, plugin_rate: f32) -> Vec<Vec<f32>> {
    if (file_rate - plugin_rate).abs() < 0.5 {
        channels
    } else {
        channels.into_iter().map(|c| resample_linear(&c, file_rate, plugin_rate)).collect()
    }
}

/// Combines `load_wav_channels` and `prepare_source_for_plugin_rate` - the
/// full "get a WAV file's audio ready to be frozen at this sample rate"
/// step, shared by the interactive "Load Sample..."/"[ Load Sample ]" flow,
/// project-recall in `initialize()`, and preset recall in `apply_preset()`.
fn load_and_prepare_sample(path: &Path, plugin_rate: f32) -> Result<Vec<Vec<f32>>, String> {
    let (channels, file_rate) = load_wav_channels(path)?;
    Ok(prepare_source_for_plugin_rate(channels, file_rate, plugin_rate))
}

/// The full "a real file has just been chosen" flow, shared by the
/// interactive file dialog (`open_sample_dialog`) and preset recall
/// (`apply_preset`, when a preset references a sample): decode/resample it,
/// swap it into `source`, persist the path (`PrismPluginParams::
/// sample_path`, see its doc comment) so it survives a host project
/// save/reload, trigger a re-render at the current Freeze Point/Formant
/// Shift/Stereo Width, and update `loaded_filename` plus any editor error
/// message.
fn load_sample_from_path(
    path: &Path,
    source: &Arc<ArcSwap<Vec<Vec<f32>>>>,
    loop_buffer: &Arc<ArcSwap<LoopBufferData>>,
    trigger: &RenderTrigger,
    params: &PrismPluginParams,
    loaded_filename: &Arc<ArcSwapOption<String>>,
    state: &mut PrismEditorState,
) {
    match load_and_prepare_sample(path, loop_buffer.load().sample_rate) {
        Ok(prepared) => {
            source.store(Arc::new(prepared));
            *params.sample_path.lock().unwrap() = Some(path.to_path_buf());
            trigger.request_render(RenderRequest {
                freeze_point_pct: params.freeze_point.value(),
                formant_shift_semitones: params.formant_shift.value(),
                stereo_width_pct: params.stereo_width.value(),
                loop_length_seconds: params.loop_length_seconds.value(),
            });
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            loaded_filename.store(name.map(Arc::new));
            state.error = None;
        }
        Err(e) => state.error = Some(e),
    }
}

/// Writes the currently-playing frozen loop (`LoopBufferData`, the actual
/// rendered spectral snapshot - not the original source sample) to a WAV
/// file, for a downloader to import into another program (e.g. OpenMPT).
/// 16-bit PCM int, not 32-bit float, to match this project's existing
/// PCM-WAV export convention (0006's own sampler/sample-ZIP exports) and
/// for the widest possible compatibility with trackers/samplers - matching
/// `prism_cli`'s own WAV writer isn't done here since that one deliberately
/// stays float (a fast-iteration DSP test harness, not a distributable
/// export), and the two have no shared dependency to justify merging over.
fn write_loop_buffer_wav(path: &Path, buffer: &LoopBufferData) -> Result<(), String> {
    let Some(left) = buffer.channels.first() else {
        return Err("nothing has been frozen yet - load a sample first".to_string());
    };
    let right = buffer.channels.get(1).unwrap_or(left);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: buffer.sample_rate as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| format!("couldn't create WAV file: {e}"))?;
    for (&l, &r) in left.iter().zip(right.iter()) {
        let l_i16 = (l.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        let r_i16 = (r.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        writer.write_sample(l_i16).map_err(|e| format!("couldn't write sample: {e}"))?;
        writer.write_sample(r_i16).map_err(|e| format!("couldn't write sample: {e}"))?;
    }
    writer.finalize().map_err(|e| format!("couldn't finalize WAV file: {e}"))
}

/// A named snapshot of every DSP-relevant param, plus the sample it was
/// made with (if any) - see `FREEZE-PLAN-010`. Deliberately doesn't include
/// `editor_state` (window geometry has nothing to do with the sound) or any
/// GUI-only state.
#[derive(serde::Serialize, serde::Deserialize)]
struct Preset {
    freeze_point_pct: f32,
    formant_shift_semitones: f32,
    stereo_width_pct: f32,
    /// `#[serde(default)]` with a custom default fn (not the bare
    /// `0.0` a plain `#[serde(default)]` would give) - unlike
    /// `pan_center_pct`/`pan_width_pct` where 0.0 is the semantically
    /// correct "off" value, 0.0 seconds is a nonsensical loop length. A
    /// preset saved before this param existed comes back at
    /// `DEFAULT_LOOP_SECONDS` instead, matching what that preset's sound
    /// actually used at the time.
    #[serde(default = "default_loop_length_seconds")]
    loop_length_seconds: f32,
    attack_ms: f32,
    decay_ms: f32,
    sustain_pct: f32,
    release_ms: f32,
    velocity_sensitivity_pct: f32,
    /// `#[serde(default)]` on these three: presets saved before they
    /// existed must still load rather than fail outright - they'll just
    /// come back with pitch bend disabled (range 0) and no panning (center/
    /// width 0), the same as a freshly-created instance would have before
    /// anyone touched these controls.
    #[serde(default)]
    pitch_bend_range_semitones: f32,
    #[serde(default)]
    pan_center_pct: f32,
    #[serde(default)]
    pan_width_pct: f32,
    /// `None` if the preset was saved while still on the built-in
    /// placeholder tone - recalling it then leaves whatever sample is
    /// already loaded untouched rather than resetting to the placeholder.
    sample_path: Option<PathBuf>,
    /// Which `BUILD_NUMBER` (YYYYMMDD) made this preset, so it's possible to
    /// tell which plugin version to blame if it doesn't sound right after
    /// importing into a later one. `#[serde(default)]` (an empty string)
    /// for presets saved before this existed - displayed as "unknown"
    /// rather than a blank/confusing value. Purely informational - never
    /// applied to anything, and never blocks an import.
    #[serde(default)]
    build_number: String,
}

fn default_loop_length_seconds() -> f32 {
    prism_dsp::render::DEFAULT_LOOP_SECONDS
}

impl Preset {
    fn capture(params: &PrismPluginParams) -> Self {
        Self {
            freeze_point_pct: params.freeze_point.value(),
            formant_shift_semitones: params.formant_shift.value(),
            stereo_width_pct: params.stereo_width.value(),
            loop_length_seconds: params.loop_length_seconds.value(),
            attack_ms: params.attack.value(),
            decay_ms: params.decay.value(),
            sustain_pct: params.sustain.value(),
            release_ms: params.release.value(),
            velocity_sensitivity_pct: params.velocity_sensitivity.value(),
            pitch_bend_range_semitones: params.pitch_bend_range_semitones.value(),
            pan_center_pct: params.pan_center_pct.value(),
            pan_width_pct: params.pan_width_pct.value(),
            sample_path: params.sample_path.lock().unwrap().clone(),
            build_number: BUILD_NUMBER.to_string(),
        }
    }
}

/// Presets live in a per-user directory rather than next to the plugin
/// bundle (which may not be writable, depending on install location),
/// created on first use. `None` if `$HOME` isn't set (or the directory
/// can't be created), which just disables preset save/load rather than
/// panicking - there's nowhere sensible to fall back to.
fn presets_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = PathBuf::from(home).join(".config").join("SpectralPrism").join("presets");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn preset_file_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.json"))
}

/// Sorted (so Prev/Next and the dropdown have a stable, predictable order)
/// list of preset names, without the `.json` extension, found in `dir`. An
/// unreadable directory (shouldn't happen once `presets_dir()` has
/// succeeded once, but e.g. permissions could change) just yields no
/// presets rather than an error - there's no interactive action to blame it
/// on, unlike an explicit save/load.
fn list_presets(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| entry.path().file_stem().map(|stem| stem.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

/// Writes a preset to an arbitrary path - shared by the named on-disk
/// library (`save_preset`, always under `presets_dir()`) and the
/// interactive "Export Preset..." file dialog (any path the user picks).
/// JSON, via the same `Preset`/serde type either route captures - see that
/// struct's `#[serde(default)]` fields for how this stays readable by
/// future plugin versions that add more parameters, and by past ones.
fn write_preset_file(path: &Path, preset: &Preset) -> Result<(), String> {
    let json = serde_json::to_string_pretty(preset).map_err(|e| format!("couldn't serialize preset: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("couldn't write preset file: {e}"))
}

/// Reads a preset from an arbitrary path - see `write_preset_file`.
fn read_preset_file(path: &Path) -> Result<Preset, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("couldn't read preset file: {e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("couldn't parse preset file: {e}"))
}

fn save_preset(dir: &Path, name: &str, preset: &Preset) -> Result<(), String> {
    write_preset_file(&preset_file_path(dir, name), preset)
}

fn load_preset(dir: &Path, name: &str) -> Result<Preset, String> {
    read_preset_file(&preset_file_path(dir, name))
}

/// GUI-thread-only state for the editor - not shared with the audio thread
/// and not persisted. Which file is loaded lives on the plugin itself
/// (`PrismPlugin::loaded_filename`) instead, so it survives the editor
/// window being closed and reopened; the selected-preset name lives on
/// `PrismPluginParams` for the same reason (and so it round-trips through a
/// saved host project - see that field's doc comment). Only the load-error
/// message and the preset-name text field (fine to forget when the editor
/// is reopened - not sound-affecting state) stay here.
#[derive(Default)]
struct PrismEditorState {
    error: Option<String>,
    /// Text field buffer for naming a new preset to save.
    preset_name_input: String,
}

/// Multiplies built-in text sizes and interactive-widget spacing by `scale`,
/// so dragging `ResizableWindow`'s corner (see `editor()` below) genuinely
/// makes buttons/sliders/labels bigger too, not just the custom-drawn
/// waveform/ADSR graphs (which already grow via `ui.available_width()`).
///
/// This is necessary rather than just calling `egui::Context::
/// set_zoom_factor()`: this project's pinned `nih_plug_egui`/`egui_baseview`
/// revision renders using its *own* `pixels_per_point` field, tracked
/// entirely inside `egui_baseview`'s window loop and reset on every host
/// resize event from the fixed `WindowScalePolicy` nih_plug_egui chose at
/// window-creation time (`Some(1.0)` on Linux) - it never reads back
/// whatever `Context::set_zoom_factor()`/`pixels_per_point()` was set to
/// internally, so that call has no visible effect on this platform/version.
/// Scaling the actual "points" sizes (fonts, spacing, and our own custom
/// draw dimensions) is the one lever that reaches every widget regardless.
///
/// Rebuilt from `egui::Style::default()`'s reference values every call
/// (not compounded onto whatever the style already is) so it stays correct
/// as the window is dragged to any size, in either direction, and applied
/// via `all_styles_mut` so it doesn't clobber dark/light `Visuals`.
fn apply_gui_scale(ctx: &egui::Context, scale: f32) {
    let base = egui::Style::default();
    ctx.all_styles_mut(|style| {
        for (text_style, font_id) in style.text_styles.iter_mut() {
            if let Some(base_font) = base.text_styles.get(text_style) {
                font_id.size = base_font.size * scale;
            }
        }
        style.spacing.item_spacing = base.spacing.item_spacing * scale;
        style.spacing.button_padding = base.spacing.button_padding * scale;
        style.spacing.interact_size = base.spacing.interact_size * scale;
        style.spacing.slider_width = base.spacing.slider_width * scale;
    });
}

/// Draws the loaded source's waveform (min/max per pixel column, since the
/// source is almost always much longer than the display is wide) with a
/// vertical marker at Freeze Point's current position, so the user can see
/// *what* they're about to freeze rather than reading a bare percentage.
/// Click/drag directly on it to set Freeze Point, using the same
/// begin/set/end-normalized pattern a built-in `ParamSlider` uses
/// internally, just driven by pixel position instead of a slider track.
///
/// Until the user has explicitly loaded a real file (`has_loaded_sample`)
/// there's nothing meaningful to show or drag - the plugin always has
/// *some* audio in `source` even then (the built-in placeholder tone,
/// loaded at `initialize()` before the editor can even open), so checking
/// whether `source` itself is empty would never actually trigger this.
/// Instead this area becomes a clickable "[ Load Sample ]" sign - returns
/// `true` on the frame it's clicked, so the caller (which owns the actual
/// file dialog / decode logic, shared with the "Load Sample..." button
/// below) can open it in response.
fn draw_freeze_point_waveform(
    ui: &mut egui::Ui,
    source: &Arc<ArcSwap<Vec<Vec<f32>>>>,
    freeze_point: &FloatParam,
    setter: &ParamSetter,
    has_loaded_sample: bool,
    scale: f32,
) -> bool {
    let desired_size = egui::vec2(ui.available_width(), GRAPH_HEIGHT * scale);
    let painter = ui.painter().clone();

    if !has_loaded_sample {
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
        let bg = if response.hovered() { COLOR_EDGE } else { COLOR_SURFACE_DEEP };
        painter.rect_filled(rect, 2.0, bg);
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, COLOR_EDGE), egui::StrokeKind::Inside);
        let text_color = if response.hovered() { COLOR_ACCENT } else { COLOR_DIM };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "[ Load Sample ]",
            egui::FontId::proportional(14.0 * scale),
            text_color,
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        return response.clicked();
    }

    let source_guard = source.load();
    let samples = source_guard.first().filter(|s| !s.is_empty());
    let Some(samples) = samples else {
        // Shouldn't happen while `has_loaded_sample` is true, but avoid a
        // panic on the empty slice below if it somehow does.
        ui.allocate_exact_size(desired_size, egui::Sense::hover());
        return false;
    };

    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
    painter.rect_filled(rect, 2.0, COLOR_SURFACE_DEEP);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, COLOR_EDGE), egui::StrokeKind::Inside);

    let width_px = (rect.width().max(1.0) as usize).max(1);
    let mid_y = rect.center().y;
    let half_height = rect.height() * 0.5 * 0.9;
    let samples_per_px = (samples.len() as f32 / width_px as f32).max(1.0);
    let waveform_stroke = egui::Color32::from_rgba_unmultiplied(COLOR_INK.r(), COLOR_INK.g(), COLOR_INK.b(), 160);
    for px in 0..width_px {
        let start = ((px as f32) * samples_per_px) as usize;
        if start >= samples.len() {
            break;
        }
        let end = (((px + 1) as f32) * samples_per_px).ceil() as usize;
        let end = end.clamp(start + 1, samples.len());
        let slice = &samples[start..end];
        let (min_v, max_v) =
            slice.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(mn, mx), &s| (mn.min(s), mx.max(s)));
        let x = rect.left() + px as f32;
        let y_top = mid_y - max_v.clamp(-1.0, 1.0) * half_height;
        let y_bottom = (mid_y - min_v.clamp(-1.0, 1.0) * half_height).max(y_top + 1.0);
        painter.line_segment([egui::pos2(x, y_top), egui::pos2(x, y_bottom)], egui::Stroke::new(1.0, waveform_stroke));
    }

    let freeze_normalized = freeze_point.unmodulated_normalized_value();
    let marker_x = rect.left() + freeze_normalized * rect.width();
    painter.line_segment(
        [egui::pos2(marker_x, rect.top()), egui::pos2(marker_x, rect.bottom())],
        egui::Stroke::new(2.0, COLOR_ACCENT),
    );

    if response.drag_started() || response.clicked() {
        setter.begin_set_parameter(freeze_point);
    }
    if response.dragged() || response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let normalized = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            setter.set_parameter_normalized(freeze_point, normalized);
        }
    }
    if response.drag_stopped() || response.clicked() {
        setter.end_set_parameter(freeze_point);
    }

    false
}

/// Draws the ADSR envelope shape with three grabbable handles instead of
/// four plain sliders: the attack-end point (drags horizontally only - it
/// always tops out at level 1.0), the decay-end/sustain-level point (drags
/// both axes), and the release-end point (drags horizontally only - it
/// always returns to level 0.0). Attack/Decay/Release each get an equal
/// fixed-width "slot" for layout (not sized by their own current value, so
/// the graph doesn't rescale itself while you're dragging it) with a fixed-
/// width flat plateau for Sustain, which has no time/duration of its own.
fn draw_adsr_graph(
    ui: &mut egui::Ui,
    attack: &FloatParam,
    decay: &FloatParam,
    sustain: &FloatParam,
    release: &FloatParam,
    setter: &ParamSetter,
    scale: f32,
) {
    let desired_size = egui::vec2(ui.available_width(), GRAPH_HEIGHT * scale);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, COLOR_SURFACE_DEEP);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, COLOR_EDGE), egui::StrokeKind::Inside);

    let sustain_plateau_width: f32 = 30.0 * scale;
    let segment_width = ((rect.width() - sustain_plateau_width) / 3.0).max(1.0);
    let margin = 10.0 * scale;
    let plot_top = rect.top() + margin;
    let plot_bottom = rect.bottom() - margin;
    let plot_height = (plot_bottom - plot_top).max(1.0);
    let y_for_level = |level: f32| plot_bottom - level.clamp(0.0, 1.0) * plot_height;
    let level_for_y = |y: f32| ((plot_bottom - y) / plot_height).clamp(0.0, 1.0);

    let attack_start_x = rect.left();
    let decay_start_x = attack_start_x + segment_width;
    let sustain_start_x = decay_start_x + segment_width;
    let release_start_x = sustain_start_x + sustain_plateau_width;

    let attack_norm = attack.unmodulated_normalized_value();
    let decay_norm = decay.unmodulated_normalized_value();
    let sustain_norm = sustain.unmodulated_normalized_value();
    let release_norm = release.unmodulated_normalized_value();

    let p0 = egui::pos2(attack_start_x, y_for_level(0.0));
    let p1 = egui::pos2(attack_start_x + segment_width * attack_norm, y_for_level(1.0));
    let p2 = egui::pos2(decay_start_x + segment_width * decay_norm, y_for_level(sustain_norm));
    let p3 = egui::pos2(sustain_start_x + sustain_plateau_width, y_for_level(sustain_norm));
    let p4 = egui::pos2(release_start_x + segment_width * release_norm, y_for_level(0.0));

    let curve_stroke = egui::Stroke::new(2.0, COLOR_ACCENT);
    painter.line_segment([p0, p1], curve_stroke);
    painter.line_segment([p1, p2], curve_stroke);
    painter.line_segment([p2, p3], curve_stroke);
    painter.line_segment([p3, p4], curve_stroke);

    let handle_radius = 5.0 * scale;
    let handle_color = COLOR_ACCENT;
    painter.circle_filled(p1, handle_radius, handle_color);
    painter.circle_filled(p2, handle_radius, handle_color);
    painter.circle_filled(p4, handle_radius, handle_color);

    let handle_rect = |center: egui::Pos2| egui::Rect::from_center_size(center, egui::vec2(handle_radius * 3.0, handle_radius * 3.0));

    // Attack handle: horizontal-only drag within its slot.
    let attack_id = ui.make_persistent_id("adsr_attack_handle");
    let attack_response = ui.interact(handle_rect(p1), attack_id, egui::Sense::click_and_drag());
    if attack_response.drag_started() || attack_response.clicked() {
        setter.begin_set_parameter(attack);
    }
    if attack_response.dragged() || attack_response.clicked() {
        if let Some(pos) = attack_response.interact_pointer_pos() {
            setter.set_parameter_normalized(attack, ((pos.x - attack_start_x) / segment_width).clamp(0.0, 1.0));
        }
    }
    if attack_response.drag_stopped() || attack_response.clicked() {
        setter.end_set_parameter(attack);
    }

    // Decay/Sustain handle: both axes at once (X -> decay time, Y -> sustain level).
    let decay_sustain_id = ui.make_persistent_id("adsr_decay_sustain_handle");
    let ds_response = ui.interact(handle_rect(p2), decay_sustain_id, egui::Sense::click_and_drag());
    if ds_response.drag_started() || ds_response.clicked() {
        setter.begin_set_parameter(decay);
        setter.begin_set_parameter(sustain);
    }
    if ds_response.dragged() || ds_response.clicked() {
        if let Some(pos) = ds_response.interact_pointer_pos() {
            setter.set_parameter_normalized(decay, ((pos.x - decay_start_x) / segment_width).clamp(0.0, 1.0));
            setter.set_parameter_normalized(sustain, level_for_y(pos.y));
        }
    }
    if ds_response.drag_stopped() || ds_response.clicked() {
        setter.end_set_parameter(decay);
        setter.end_set_parameter(sustain);
    }

    // Release handle: horizontal-only drag within its slot.
    let release_id = ui.make_persistent_id("adsr_release_handle");
    let release_response = ui.interact(handle_rect(p4), release_id, egui::Sense::click_and_drag());
    if release_response.drag_started() || release_response.clicked() {
        setter.begin_set_parameter(release);
    }
    if release_response.dragged() || release_response.clicked() {
        if let Some(pos) = release_response.interact_pointer_pos() {
            setter.set_parameter_normalized(release, ((pos.x - release_start_x) / segment_width).clamp(0.0, 1.0));
        }
    }
    if release_response.drag_stopped() || release_response.clicked() {
        setter.end_set_parameter(release);
    }
}

impl Plugin for PrismPlugin {
    const NAME: &'static str = "SpectralPrism";
    const VENDOR: &'static str = "John Oestmann";
    const URL: &'static str = "https://johnoestmannmusic.com";
    const EMAIL: &'static str = "contact@johnoestmannmusic.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    // MidiCCs (not just Basic) is required to actually receive
    // NoteEvent::MidiPitchBend - Basic alone silently never delivers it.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let source = self.source.clone();
        let loop_buffer = self.loop_buffer.clone();
        let trigger = self.trigger.clone();
        let last_note = self.last_note.clone();
        let active_voice_count = self.active_voice_count.clone();
        let loaded_filename = self.loaded_filename.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            PrismEditorState::default(),
            |ctx, _state| apply_theme(ctx),
            move |egui_ctx, setter, state| {
                // GUI scaling: the corner of `ResizableWindow` below lets the
                // user drag the window to any size (through nih-plug's real
                // host-negotiated resize, not just adding blank space); here
                // we read back the *actual* current size and derive how much
                // bigger than the base design size it now is, then scale
                // every widget's actual "points" dimensions by that (see
                // `apply_gui_scale` for why - not `set_zoom_factor`, which
                // this nih_plug_egui/egui_baseview revision doesn't apply at
                // render time). Never scales below 1x (`min_size` below also
                // stops the window from being dragged smaller than the base
                // size in the first place).
                let (current_width, current_height) = params.editor_state.size();
                let scale = (((current_width as f32 / BASE_EDITOR_WIDTH as f32)
                    + (current_height as f32 / BASE_EDITOR_HEIGHT as f32))
                    / 2.0)
                    .max(1.0);
                apply_gui_scale(egui_ctx, scale);

                // Shared by the waveform-area "[ Load Sample ]" sign (shown
                // before anything is loaded) and the "Load Sample..." button
                // below it (always available) - both just need to trigger the
                // same file dialog / decode / re-render flow.
                let open_sample_dialog = |state: &mut PrismEditorState| {
                    if let Some(path) = rfd::FileDialog::new().add_filter("WAV", &["wav", "WAV"]).pick_file() {
                        load_sample_from_path(&path, &source, &loop_buffer, &trigger, &params, &loaded_filename, state);
                    }
                };

                // Recalls a saved snapshot: every DSP param via the same
                // begin/set/end pattern the custom graph widgets use (so
                // host automation recording sees a proper set, not a silent
                // jump), plus the referenced sample if the preset has one -
                // sharing `load_sample_from_path` with the interactive
                // dialog so both paths persist `sample_path` and re-render
                // identically. A preset saved without a sample (`None`)
                // leaves whatever's currently loaded untouched.
                //
                // Presets deliberately still store the sample's full
                // absolute path (privacy tradeoff accepted, since stripping
                // it would break the common case of reloading your *own*
                // presets) - but a shared preset's path naturally won't
                // resolve on someone else's machine. Rather than just
                // failing, a missing sample prompts the user to locate it
                // via a file dialog; if they do, that corrected path is
                // returned here so the caller can write it back into
                // whichever preset file this came from, so it doesn't have
                // to be re-located every time on this machine specifically.
                let apply_preset = |preset: &Preset, state: &mut PrismEditorState| -> Option<PathBuf> {
                    let set = |param: &FloatParam, value: f32| {
                        setter.begin_set_parameter(param);
                        setter.set_parameter(param, value);
                        setter.end_set_parameter(param);
                    };
                    set(&params.freeze_point, preset.freeze_point_pct);
                    set(&params.formant_shift, preset.formant_shift_semitones);
                    set(&params.stereo_width, preset.stereo_width_pct);
                    set(&params.loop_length_seconds, preset.loop_length_seconds);
                    set(&params.attack, preset.attack_ms);
                    set(&params.decay, preset.decay_ms);
                    set(&params.sustain, preset.sustain_pct);
                    set(&params.release, preset.release_ms);
                    set(&params.velocity_sensitivity, preset.velocity_sensitivity_pct);
                    set(&params.pitch_bend_range_semitones, preset.pitch_bend_range_semitones);
                    set(&params.pan_center_pct, preset.pan_center_pct);
                    set(&params.pan_width_pct, preset.pan_width_pct);

                    let Some(path) = &preset.sample_path else { return None };
                    load_sample_from_path(path, &source, &loop_buffer, &trigger, &params, &loaded_filename, state);
                    if state.error.is_none() {
                        return None;
                    }

                    rfd::MessageDialog::new()
                        .set_title("Sample not found")
                        .set_description(format!(
                            "This preset's sample couldn't be found:\n{}\n\nLocate it to continue.",
                            path.display()
                        ))
                        .show();
                    let relocated = rfd::FileDialog::new().add_filter("WAV", &["wav", "WAV"]).pick_file()?;
                    load_sample_from_path(&relocated, &source, &loop_buffer, &trigger, &params, &loaded_filename, state);
                    state.error.is_none().then_some(relocated)
                };

                // A persistent bottom strip, added *before* the central
                // content below (egui panels must be added before
                // `CentralPanel` - which `ResizableWindow` uses internally -
                // so it can shrink to leave room) so the build number stays
                // fixed in the corner regardless of the content's own
                // scroll position, rather than just being the last thing in
                // the scrollable area.
                egui::TopBottomPanel::bottom("spectral_prism_build_number")
                    .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric((6.0 * scale) as i8, (3.0 * scale) as i8)))
                    .show_separator_line(false)
                    .show(egui_ctx, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!("Build {BUILD_NUMBER}")).small().color(COLOR_DIM));
                        });
                    });

                ResizableWindow::new("spectral_prism_window")
                    .min_size(egui::vec2(BASE_EDITOR_WIDTH as f32, BASE_EDITOR_HEIGHT as f32))
                    .show(egui_ctx, &params.editor_state, |ui| {
                        egui::Frame::default().inner_margin(egui::Margin::same((EDITOR_MARGIN * scale) as i8)).show(ui, |ui| {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.heading("SpectralPrism");

                        let presets_dir = presets_dir();
                        let preset_names = presets_dir.as_deref().map(list_presets).unwrap_or_default();
                        // Cloned out of the lock immediately - held only
                        // long enough to read, not across the rest of the
                        // frame (see `PrismPluginParams::selected_preset`'s
                        // doc comment for why this lives there, not in
                        // `PrismEditorState`).
                        let selected_preset_name = params.selected_preset.lock().unwrap().clone();
                        let current_preset_idx =
                            selected_preset_name.as_ref().and_then(|name| preset_names.iter().position(|n| n == name));

                        let load_preset_at = |idx: usize, state: &mut PrismEditorState| {
                            let (Some(name), Some(dir)) = (preset_names.get(idx), &presets_dir) else { return };
                            match load_preset(dir, name) {
                                Ok(mut preset) => {
                                    // `apply_preset` already leaves `state.error`
                                    // in the right final state (set if the
                                    // sample couldn't be found/relocated,
                                    // cleared otherwise) - not overwritten here.
                                    if let Some(relocated) = apply_preset(&preset, state) {
                                        preset.sample_path = Some(relocated);
                                        // Best-effort: a failed resave isn't
                                        // worth surfacing as an error - the
                                        // preset still applied correctly this
                                        // time, it just won't remember the fix
                                        // for next time.
                                        let _ = save_preset(dir, name, &preset);
                                    }
                                    *params.selected_preset.lock().unwrap() = Some(name.clone());
                                    // Prefills the Save field with the just-
                                    // loaded preset's own name, so clicking
                                    // Save immediately overwrites it instead
                                    // of requiring the name to be retyped.
                                    state.preset_name_input = name.clone();
                                }
                                Err(e) => state.error = Some(e),
                            }
                        };

                        ui.horizontal(|ui| {
                            if ui.add_enabled(!preset_names.is_empty(), egui::Button::new("◀")).clicked() {
                                let idx = current_preset_idx.map(|i| i.saturating_sub(1)).unwrap_or(0);
                                load_preset_at(idx, state);
                            }
                            let combo_label = selected_preset_name.as_deref().unwrap_or("(no preset)");
                            egui::ComboBox::from_id_salt("spectral_prism_preset_combo").selected_text(combo_label).show_ui(
                                ui,
                                |ui| {
                                    for (idx, name) in preset_names.iter().enumerate() {
                                        if ui.selectable_label(current_preset_idx == Some(idx), name).clicked() {
                                            load_preset_at(idx, state);
                                        }
                                    }
                                },
                            );
                            if ui.add_enabled(!preset_names.is_empty(), egui::Button::new("▶")).clicked() {
                                let idx = current_preset_idx
                                    .map(|i| (i + 1).min(preset_names.len() - 1))
                                    .unwrap_or(0);
                                load_preset_at(idx, state);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.preset_name_input)
                                    .hint_text("Preset name..."),
                            );
                            let name = state.preset_name_input.trim().to_string();
                            if ui.add_enabled(!name.is_empty(), egui::Button::new("Save")).clicked() {
                                match &presets_dir {
                                    Some(dir) => match save_preset(dir, &name, &Preset::capture(&params)) {
                                        Ok(()) => {
                                            *params.selected_preset.lock().unwrap() = Some(name);
                                            state.error = None;
                                        }
                                        Err(e) => state.error = Some(e),
                                    },
                                    None => state.error = Some("couldn't find a presets directory ($HOME not set?)".to_string()),
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            // Separate from the named on-disk library above
                            // (Save/◀/▶/combo, all under `presets_dir()`) -
                            // these go to/from any file the user picks, for
                            // sharing a preset outside this machine (e.g.
                            // alongside a sample-pack WAV export, see
                            // "Export WAV Sample..." below).
                            if ui.button("Import Preset...").clicked() {
                                if let Some(path) =
                                    rfd::FileDialog::new().add_filter("SpectralPrism Preset", &["json"]).pick_file()
                                {
                                    match read_preset_file(&path) {
                                        Ok(mut preset) => {
                                            // See `load_preset_at` for why
                                            // `state.error` isn't touched here.
                                            if let Some(relocated) = apply_preset(&preset, state) {
                                                preset.sample_path = Some(relocated);
                                                let _ = write_preset_file(&path, &preset);
                                            }
                                            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
                                            // Prefills the Save field the same
                                            // way `load_preset_at` does, so
                                            // Save immediately overwrites a
                                            // re-imported preset without
                                            // retyping its name - only
                                            // meaningful for a preset saved
                                            // back into the named on-disk
                                            // library, not this arbitrary file
                                            // path, but that's exactly what
                                            // typing a name into that field
                                            // and clicking Save already does.
                                            if let Some(stem) = &stem {
                                                state.preset_name_input = stem.clone();
                                            }
                                            *params.selected_preset.lock().unwrap() = stem;
                                        }
                                        Err(e) => state.error = Some(e),
                                    }
                                }
                            }
                            if ui.button("Export Preset...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("SpectralPrism Preset", &["json"])
                                    .set_file_name("preset.json")
                                    .save_file()
                                {
                                    match write_preset_file(&path, &Preset::capture(&params)) {
                                        Ok(()) => state.error = None,
                                        Err(e) => state.error = Some(e),
                                    }
                                }
                            }
                        });

                        let note = last_note.load(Ordering::Relaxed);
                        let voice_count = active_voice_count.load(Ordering::Relaxed);
                        let note_label = if note == NO_NOTE { "--".to_string() } else { note.to_string() };
                        ui.colored_label(COLOR_DIM, format!("MIDI: note {note_label} | {voice_count} voice(s) active"));

                        ui.add_space(8.0);

                        // The two main sections side by side rather than
                        // stacked - requested directly, and it also keeps
                        // the window from getting extremely tall now that
                        // everything renders at `DEFAULT_SCALE`.
                        ui.columns(2, |columns| {
                            let left = &mut columns[0];
                            left.label("Freeze Point");
                            let has_loaded_sample = loaded_filename.load().is_some();
                            if draw_freeze_point_waveform(left, &source, &params.freeze_point, setter, has_loaded_sample, scale) {
                                open_sample_dialog(state);
                            }
                            left.add(widgets::ParamSlider::for_param(&params.freeze_point, setter));

                            left.label("Formant Shift");
                            left.add(widgets::ParamSlider::for_param(&params.formant_shift, setter));

                            left.label("Stereo Width");
                            left.add(widgets::ParamSlider::for_param(&params.stereo_width, setter));

                            left.label("Loop Length");
                            left.add(widgets::ParamSlider::for_param(&params.loop_length_seconds, setter));

                            let right = &mut columns[1];
                            right.label("Envelope (Attack / Decay / Sustain / Release)");
                            draw_adsr_graph(right, &params.attack, &params.decay, &params.sustain, &params.release, setter, scale);
                            right.label("Attack");
                            right.add(widgets::ParamSlider::for_param(&params.attack, setter));
                            right.label("Decay");
                            right.add(widgets::ParamSlider::for_param(&params.decay, setter));
                            right.label("Sustain");
                            right.add(widgets::ParamSlider::for_param(&params.sustain, setter));
                            right.label("Release");
                            right.add(widgets::ParamSlider::for_param(&params.release, setter));

                            right.add_space(8.0);
                            right.label("Velocity Sensitivity");
                            right.add(widgets::ParamSlider::for_param(&params.velocity_sensitivity, setter));

                            right.add_space(8.0);
                            right.label("Pitch Bend Range");
                            right.add(widgets::ParamSlider::for_param(&params.pitch_bend_range_semitones, setter));

                            right.add_space(8.0);
                            right.label("Pan Center");
                            right.add(widgets::ParamSlider::for_param(&params.pan_center_pct, setter));
                            right.label("Random Pan Width");
                            right.add(widgets::ParamSlider::for_param(&params.pan_width_pct, setter));
                        });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            if ui.button("Load Sample...").clicked() {
                                open_sample_dialog(state);
                            }
                            if ui.button("Export WAV Sample...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("WAV", &["wav"])
                                    .set_file_name("SpectralPrism-export.wav")
                                    .save_file()
                                {
                                    match write_loop_buffer_wav(&path, &loop_buffer.load()) {
                                        Ok(()) => state.error = None,
                                        Err(e) => state.error = Some(e),
                                    }
                                }
                            }
                        });

                        ui.add_space(4.0);
                        match loaded_filename.load().as_deref() {
                            Some(name) => {
                                ui.colored_label(COLOR_DIM, format!("Loaded: {name}"));
                            }
                            None => {
                                ui.colored_label(COLOR_DIM, "Using built-in placeholder tone");
                            }
                        }
                        if let Some(error) = &state.error {
                            ui.colored_label(COLOR_ERROR, error);
                        }
                        });
                        });
                    });
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let sample_rate = buffer_config.sample_rate;

        // `sample_path` (if any) is restored from persisted state before
        // `initialize()` runs, same as every `FloatParam` - so a saved host
        // project or a recalled preset (see `FREEZE-PLAN-010`) gets its
        // actual sample back too, not just the built-in placeholder tone.
        // A missing/moved file falls back to the placeholder rather than
        // failing outright; there's no editor open yet to show an error to,
        // so this is logged instead.
        let restored_path = self.params.sample_path.lock().unwrap().clone();
        match restored_path.as_deref().map(|path| (path, load_and_prepare_sample(path, sample_rate))) {
            Some((path, Ok(prepared))) => {
                self.source.store(Arc::new(prepared));
                self.loaded_filename.store(path.file_name().map(|n| Arc::new(n.to_string_lossy().into_owned())));
            }
            Some((path, Err(e))) => {
                nih_log!("SpectralPrism: couldn't restore sample from {}: {e}", path.display());
                self.source.store(Arc::new(vec![synthetic_source(sample_rate, 1.0)]));
                self.loaded_filename.store(None);
            }
            None => {
                self.source.store(Arc::new(vec![synthetic_source(sample_rate, 1.0)]));
                self.loaded_filename.store(None);
            }
        }

        let request = RenderRequest {
            freeze_point_pct: self.params.freeze_point.value(),
            formant_shift_semitones: self.params.formant_shift.value(),
            stereo_width_pct: self.params.stereo_width.value(),
            loop_length_seconds: self.params.loop_length_seconds.value(),
        };
        // First render happens synchronously here (initialize() runs before
        // playback starts, so blocking is fine) so process() never sees the
        // placeholder silent buffer once the host actually starts playing.
        self.loop_buffer.store(Arc::new(render_frozen_loop(
            &self.source.load(),
            sample_rate,
            request.freeze_point_pct,
            request.formant_shift_semitones,
            request.stereo_width_pct,
            request.loop_length_seconds,
            DEFAULT_ROOT_NOTE,
        )));
        self.last_requested = request;

        self.worker = Some(RenderWorker::spawn(
            self.trigger.clone(),
            self.source.clone(),
            sample_rate,
            DEFAULT_ROOT_NOTE,
            self.loop_buffer.clone(),
        ));
        self.voices = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        self.sample_rate = sample_rate;
        self.pending_request = None;
        self.throttle_countdown = 0;

        true
    }

    fn reset(&mut self) {
        self.voices.choke_all();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Freeze Point / Formant Shift / Stereo Width are checked once per
        // block (not per-sample - they're not audio-rate, and re-rendering
        // the frozen loop is too expensive to consider on every sample
        // anyway). A detected change is only actually sent to the worker
        // once every RENDER_THROTTLE_MS, so a smooth drag/automation sweep
        // can't land a new buffer swap before the previous one's crossfade
        // has finished (see RENDER_THROTTLE_MS for why that matters).
        let current_request = RenderRequest {
            freeze_point_pct: self.params.freeze_point.value(),
            formant_shift_semitones: self.params.formant_shift.value(),
            stereo_width_pct: self.params.stereo_width.value(),
            loop_length_seconds: self.params.loop_length_seconds.value(),
        };
        if current_request != self.last_requested {
            self.pending_request = Some(current_request);
        }

        self.throttle_countdown -= buffer.samples() as i64;
        if self.throttle_countdown <= 0 {
            if let Some(request) = self.pending_request.take() {
                self.trigger.request_render(request);
                self.last_requested = request;
            }
            self.throttle_countdown = ((RENDER_THROTTLE_MS / 1000.0) * self.sample_rate) as i64;
        }

        // Attack/Decay/Sustain/Release are cheap (per-sample, not
        // render-time like the three above) - no worker/crossfade/throttle
        // needed, just apply the current values to any voices triggered
        // from this point on. Like most synths, a change here doesn't
        // reshape a note already mid-envelope - only affects new note-ons.
        self.voices.set_adsr(AdsrSettings {
            attack_ms: self.params.attack.value(),
            decay_ms: self.params.decay.value(),
            sustain_level: self.params.sustain.value() / 100.0,
            release_ms: self.params.release.value(),
        });
        self.voices.set_velocity_sensitivity(self.params.velocity_sensitivity.value() / 100.0);
        self.voices.set_pan_settings(self.params.pan_center_pct.value() / 100.0, self.params.pan_width_pct.value() / 100.0);

        // Block-level MIDI handling: every event pending for this buffer is
        // applied before rendering, rather than split at the exact sample it
        // arrived on. Good enough for proving polyphony/pitch/voice-stealing
        // here; sample-accurate note timing can be revisited later if a
        // fast arpeggio/chord attack audibly needs it.
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, channel, velocity, voice_id, .. } => {
                    self.voices.note_on(note, channel, velocity, voice_id.unwrap_or(note as i32));
                    self.last_note.store(note, Ordering::Relaxed);
                }
                NoteEvent::NoteOff { note, channel, .. } => {
                    self.voices.note_off(note, channel);
                }
                NoteEvent::Choke { .. } => {
                    self.voices.choke_all();
                }
                NoteEvent::MidiPitchBend { value, .. } => {
                    // `value` is normalized [0, 1] with 0.5 = no bend (see
                    // nih-plug's own doc comment on the variant) - stored
                    // raw and converted to semitones below, after the loop,
                    // so a same-block wheel movement still takes effect for
                    // this block rather than being delayed to the next one.
                    self.pitch_bend_normalized = value;
                }
                _ => (),
            }
        }
        self.voices.set_pitch_bend_semitones(
            (self.pitch_bend_normalized - 0.5) * 2.0 * self.params.pitch_bend_range_semitones.value(),
        );

        let loop_buffer = self.loop_buffer.load();
        let channels = buffer.as_slice();
        let (left, right) = channels.split_at_mut(1);
        self.voices.process_block(&loop_buffer, left[0], right[0]);
        // Read *after* process_block so a voice that finished this block is
        // already reflected, not the stale pre-block count.
        self.active_voice_count.store(self.voices.active_voice_count() as u8, Ordering::Relaxed);

        ProcessStatus::Normal
    }
}

impl ClapPlugin for PrismPlugin {
    const CLAP_ID: &'static str = "com.johnoestmannmusic.spectral-prism";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Freezes a spectral snapshot of a sample into a sustained pad/drone");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for PrismPlugin {
    // Deliberately different from the old "SpectralFreeze01" - this is a
    // rename (SpectralPrism), not just a relabeling, and a new class ID is
    // the correct way to signal that to a host (a host distinguishes VST3
    // plugins by this ID, not by name). Any already-saved project
    // referencing the old ID will need the renamed plugin re-added.
    const VST3_CLASS_ID: [u8; 16] = *b"SpectralPrism001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(PrismPlugin);
nih_export_vst3!(PrismPlugin);

#[cfg(test)]
mod preset_tests {
    use super::*;

    /// A fresh, empty directory under the OS temp dir, unique per test run
    /// (no `tempfile` dependency needed for this small a use). Not cleaned
    /// up automatically - left for the OS/user to reclaim, same as any
    /// other stray `/tmp` file, since these tests don't run often enough
    /// for that to matter.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "spectralprism_test_{name}_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    fn sample_preset(sample_path: Option<PathBuf>) -> Preset {
        Preset {
            freeze_point_pct: 42.0,
            formant_shift_semitones: -3.5,
            stereo_width_pct: 60.0,
            loop_length_seconds: 2.5,
            attack_ms: 12.0,
            decay_ms: 250.0,
            sustain_pct: 70.0,
            release_ms: 500.0,
            velocity_sensitivity_pct: 80.0,
            pitch_bend_range_semitones: 4.0,
            pan_center_pct: -10.0,
            pan_width_pct: 35.0,
            sample_path,
            build_number: "20260101".to_string(),
        }
    }

    #[test]
    fn preset_round_trips_through_json() {
        let original = sample_preset(Some(PathBuf::from("/some/sample.wav")));
        let json = serde_json::to_string(&original).unwrap();
        let restored: Preset = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.freeze_point_pct, original.freeze_point_pct);
        assert_eq!(restored.formant_shift_semitones, original.formant_shift_semitones);
        assert_eq!(restored.stereo_width_pct, original.stereo_width_pct);
        assert_eq!(restored.loop_length_seconds, original.loop_length_seconds);
        assert_eq!(restored.attack_ms, original.attack_ms);
        assert_eq!(restored.decay_ms, original.decay_ms);
        assert_eq!(restored.sustain_pct, original.sustain_pct);
        assert_eq!(restored.release_ms, original.release_ms);
        assert_eq!(restored.velocity_sensitivity_pct, original.velocity_sensitivity_pct);
        assert_eq!(restored.pitch_bend_range_semitones, original.pitch_bend_range_semitones);
        assert_eq!(restored.pan_center_pct, original.pan_center_pct);
        assert_eq!(restored.pan_width_pct, original.pan_width_pct);
        assert_eq!(restored.sample_path, original.sample_path);
        assert_eq!(restored.build_number, original.build_number);
    }

    #[test]
    fn preset_without_pitch_bend_or_pan_fields_still_deserializes() {
        // Regression test for backward compatibility: a preset saved before
        // FREEZE-PLAN-018 added these fields must still load, defaulting to
        // pitch bend disabled and no panning (see the `Preset` struct's doc
        // comment on why 0.0 is an acceptable fallback for both); before
        // FREEZE-PLAN-023 added build_number, defaulting to an empty string
        // (displayed as "unknown", never applied to anything); and before
        // FREEZE-PLAN-024 added loop_length_seconds, defaulting to
        // DEFAULT_LOOP_SECONDS rather than 0.0 (see that field's doc
        // comment on why 0.0 would be nonsensical here).
        let old_json = r#"{
            "freeze_point_pct": 42.0,
            "formant_shift_semitones": -3.5,
            "stereo_width_pct": 60.0,
            "attack_ms": 12.0,
            "decay_ms": 250.0,
            "sustain_pct": 70.0,
            "release_ms": 500.0,
            "velocity_sensitivity_pct": 80.0,
            "sample_path": null
        }"#;
        let restored: Preset = serde_json::from_str(old_json).expect("old-format preset should still deserialize");
        assert_eq!(restored.pitch_bend_range_semitones, 0.0);
        assert_eq!(restored.pan_center_pct, 0.0);
        assert_eq!(restored.pan_width_pct, 0.0);
        assert_eq!(restored.build_number, "");
        assert_eq!(restored.loop_length_seconds, prism_dsp::render::DEFAULT_LOOP_SECONDS);
    }

    #[test]
    fn save_then_load_preset_round_trips_on_disk() {
        let dir = temp_dir("save_load");
        let original = sample_preset(None);

        save_preset(&dir, "My Test Preset", &original).expect("save should succeed");
        let restored = load_preset(&dir, "My Test Preset").expect("load should succeed");

        assert_eq!(restored.freeze_point_pct, original.freeze_point_pct);
        assert_eq!(restored.sample_path, None);
    }

    #[test]
    fn list_presets_returns_sorted_names_without_extension() {
        let dir = temp_dir("list");
        save_preset(&dir, "Zebra", &sample_preset(None)).unwrap();
        save_preset(&dir, "Alpha", &sample_preset(None)).unwrap();
        save_preset(&dir, "Mango", &sample_preset(None)).unwrap();
        // A non-JSON file in the same directory should be ignored.
        std::fs::write(dir.join("notes.txt"), "not a preset").unwrap();

        let names = list_presets(&dir);

        assert_eq!(names, vec!["Alpha".to_string(), "Mango".to_string(), "Zebra".to_string()]);
    }

    #[test]
    fn list_presets_on_missing_directory_returns_empty() {
        let dir = std::env::temp_dir().join("spectralprism_test_definitely_does_not_exist");
        assert!(list_presets(&dir).is_empty());
    }

    #[test]
    fn load_preset_with_unknown_name_fails_with_message() {
        let dir = temp_dir("missing_preset");
        let result = load_preset(&dir, "does_not_exist");
        assert!(result.is_err());
    }

    #[test]
    fn write_then_read_preset_file_round_trips_at_an_arbitrary_path() {
        // The file-dialog Import/Export flow, as opposed to save_preset/
        // load_preset's fixed presets_dir()-relative naming.
        let dir = temp_dir("preset_file");
        let path = dir.join("my-shared-preset.json");
        let original = sample_preset(Some(PathBuf::from("/some/sample.wav")));

        write_preset_file(&path, &original).expect("write should succeed");
        let restored = read_preset_file(&path).expect("read should succeed");

        assert_eq!(restored.freeze_point_pct, original.freeze_point_pct);
        assert_eq!(restored.pan_width_pct, original.pan_width_pct);
        assert_eq!(restored.sample_path, original.sample_path);
    }

    #[test]
    fn write_loop_buffer_wav_produces_a_readable_stereo_wav() {
        let dir = temp_dir("wav_export");
        let path = dir.join("export.wav");
        let buffer = LoopBufferData {
            channels: vec![vec![0.5f32; 100], vec![-0.25f32; 100]],
            sample_rate: 48000.0,
            root_note: DEFAULT_ROOT_NOTE,
        };

        write_loop_buffer_wav(&path, &buffer).expect("export should succeed");

        let mut reader = hound::WavReader::open(&path).expect("exported file should be a valid WAV");
        let spec = reader.spec();
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, 48000);
        let samples: Vec<i32> = reader.samples::<i32>().map(|s| s.unwrap()).collect();
        // Interleaved L/R - spot-check the first frame rather than every
        // sample, converting back from i16 to approximately the original
        // f32 range to allow for quantization rounding.
        assert!((samples[0] as f32 / i16::MAX as f32 - 0.5).abs() < 1e-3);
        assert!((samples[1] as f32 / i16::MAX as f32 - -0.25).abs() < 1e-3);
    }

    #[test]
    fn write_loop_buffer_wav_fails_with_message_when_nothing_frozen() {
        let dir = temp_dir("wav_export_empty");
        let path = dir.join("export.wav");
        let buffer = LoopBufferData { channels: Vec::new(), sample_rate: 48000.0, root_note: DEFAULT_ROOT_NOTE };
        assert!(write_loop_buffer_wav(&path, &buffer).is_err());
    }
}
