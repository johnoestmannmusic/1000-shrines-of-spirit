//! The Insights shown in the ticker. Each is tagged with the panel it
//! describes (that panel lights up while its insight is showing) and with
//! when it applies, so a version only ever shows insights that are true for
//! it. `{kit}`, `{key}`, `{scale}`, `{chords}`, `{recipe}` and the tempo
//! placeholders (`{bpm}`, `{half}`, `{exact}`, `{bar_half}`, `{bar_jungle}`,
//! `{drum16}`, `{glitch16}`, `{echo_ms}`, `{tape_ms}`) are filled in with
//! this version's values.

use super::Panel::{self, *};
use shrine0011::atmos::LoopTimbre;
use shrine0011::harmony::Pace;
use shrine0011::kits::{DrumSpace, Kit};
use shrine0011::tempo::Tempo;
use shrine0011::{DroneArc, LoopDesign};
use shrine0011::SAMPLE_RATE;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum When {
    Always,
    /// Only when this kit is playing.
    Kit(Kit),
    /// Whenever there are drums.
    Drums,
    /// Only when the kit is Off.
    NoDrums,
    /// The light, minimal kits.
    LightKit,
    /// The older, heavier kits.
    HeavyKit,
    /// Drums panned per hit (Wide or Wide + tape echo).
    Panned,
    /// Drums with the tape echo.
    TapeEcho,
    /// Whenever an atmosphere loop plays.
    Loops,
    /// Only when this loop timbre is playing.
    Loop(LoopTimbre),
    /// When there are no atmosphere loops.
    NoLoops,
    /// When both atmosphere loops play.
    BothLoops,
    /// When the drone is gated by a repeating entrance/exit arc.
    DroneArc,
    /// When at least one atmosphere layer is an editable event grid.
    EventLoops,
}

impl When {
    pub fn applies(self, kit: Kit, space: DrumSpace, loops: [LoopDesign; 2], drone: DroneArc) -> bool {
        let drums = kit != Kit::Off;
        let any_loop = loops.iter().any(|d| d.is_on());
        match self {
            When::Loops => any_loop,
            When::Loop(t) => loops.iter().any(|d| d.timbre() == Some(t)),
            When::NoLoops => !any_loop,
            When::BothLoops => loops.iter().all(|d| d.is_on()),
            When::EventLoops => loops.iter().any(|d| matches!(d, LoopDesign::Events(_))),
            When::DroneArc => drone.is_on(),
            When::Always => true,
            When::Kit(k) => k == kit,
            When::Drums => drums,
            When::NoDrums => !drums,
            When::LightKit => kit.is_light(),
            When::HeavyKit => drums && !kit.is_light(),
            When::Panned => drums && space != DrumSpace::Centred,
            When::TapeEcho => drums && space == DrumSpace::Tape,
        }
    }
}

/// This version's values, for the placeholders.
pub struct Vars {
    pub kit: String,
    pub key: String,
    pub scale: String,
    pub chords: String,
    pub recipe: String,
    pub tempo: Tempo,
    /// Each loop's timbre and length (e.g. "8 beats, 5.7 s"), and when the two line up again.
    pub loop1: String,
    pub loop2: String,
    pub loop1_len: String,
    pub loop2_len: String,
    pub loops_meet: String,
    /// The loops that play, e.g. "loop 1 Choir \"Aah\" and loop 2 Glass", and their lengths.
    pub loops: String,
    pub loops_len: String,
    /// Where the root note comes from, for the key insight.
    pub key_source: String,
    /// The drone arc, e.g. "every 120 s the drone swells up for 40 s and then leaves for 80 s".
    pub drone_arc: String,
    pub drone_cycle: String,
    pub drone_hold: String,
}

/// "8 beats, 5.7 s" for a loop layer.
pub fn loop_len(l: &shrine0011::telemetry::LoopInfo, sample_rate: u32) -> String {
    format!("{} beats, {:.1} s", l.beats, l.len as f64 / sample_rate as f64)
}

pub fn fill(text: &str, v: &Vars) -> String {
    let t = v.tempo;
    let secs = |samples: u64| samples as f64 / SAMPLE_RATE as f64;
    let ms = |samples: u64| format!("{:.0} ms", 1000.0 * secs(samples));
    let num = |n: u64| {
        let s = n.to_string();
        if n >= 1000 { format!("{},{}", &s[..s.len() - 3], &s[s.len() - 3..]) } else { s }
    };
    text.replace("{kit}", &v.kit)
        .replace("{key}", &v.key)
        .replace("{scale}", &v.scale)
        .replace("{chords}", &v.chords)
        .replace("{recipe}", &v.recipe)
        .replace("{loop1}", &v.loop1)
        .replace("{loop1_len}", &v.loop1_len)
        .replace("{loop2}", &v.loop2)
        .replace("{loop2_len}", &v.loop2_len)
        .replace("{loops_meet}", &v.loops_meet)
        .replace("{loops_len}", &v.loops_len)
        .replace("{loops}", &v.loops)
        .replace("{key_source}", &v.key_source)
        .replace("{drone_arc}", &v.drone_arc)
        .replace("{drone_cycle}", &v.drone_cycle)
        .replace("{drone_hold}", &v.drone_hold)
        .replace("{bpm}", &t.bpm.to_string())
        .replace("{half}", &format!("{:.0}", t.half_bpm()))
        .replace("{exact}", &format!("{:.2}", t.exact_bpm()))
        .replace("{bar_half}", &format!("{:.2} s", secs(Pace::HalfTime.bar_samples(t.beat()))))
        .replace("{bar_jungle}", &format!("{:.2} s", secs(Pace::Jungle.bar_samples(t.beat()))))
        .replace("{drum16}", &num(t.drum16))
        .replace("{glitch16}", &num(t.glitch16()))
        .replace("{echo_ms}", &ms(3 * t.glitch16()))
        .replace("{tape_ms}", &ms(3 * t.drum16))
}

pub const EXPLAINERS: &[(Panel, When, &str)] = &[
    // The piece as a whole.
    (General, When::Always, "0011 is not a recording. It is a program that composes and synthesises the music live, one sample at a time, forever."),
    (Flow, When::Always, "Press Tab to switch views: 1 pipeline (the sound travelling through each chain), 2 engine (the drone, glitches and effects opened up), 3 rhythm & harmony (chords and drums), 4 learn (how to make this yourself)."),
    (Flow, When::Always, "In Learn, the Instruments lessons solo one layer at a time while explaining it. Press s to switch between the layer alone and the full mix, to hear how it fits."),
    (General, When::Always, "Soloing only changes what you hear. The engine still makes the full mix underneath, so a recording always gets the whole piece, byte for byte the same as a render."),
    (Flow, When::Always, "View 4, Learn, walks through how this piece is made, lesson by lesson, with this version's real settings, so you could rebuild it in your own studio. Use the up and down arrows there to choose a lesson."),
    (General, When::Always, "This version's recipe is {recipe}. Paste it into --recipe to hear exactly this again."),
    (General, When::Always, "Every sample is computed from six seeds, a handful of choices (tempo, scale, chord count, chord pace, drum kit, loop timbre) and a counter. The same inputs give the exact same music on any computer, now or in 20 years."),
    (General, When::Always, "Seed 1 sets the key and builds the chords, seeds 2 and 3 shape the two glitch layers, seed 4 the bass, seed 5 the chosen drum kit's sounds and break, and seed 6 the atmosphere loops. Choosing the kit 'Off' leaves the drums out entirely."),
    // Tempo.
    (General, When::Always, "Tempo is the first choice at setup. This version runs its drums at {bpm} BPM, and everything ambient (the glitch grid, chord bars, echoes) at exactly half, {half} BPM, so the two layers of time always line up."),
    (General, When::Always, "Every rhythm here is a whole number of samples, so it can never drift. A drum sixteenth is rounded to a multiple of 12 samples ({drum16}) so glitch ratchets of 2, 3 and 4 divide it exactly. The tempo you actually hear is {exact} BPM."),
    (Cycles, When::Always, "The slow form cycles (41 to 307 s) are measured in seconds, not beats, so changing the BPM moves the rhythm but leaves the long breathing of the piece alone."),
    // Atmosphere loops.
    (Loops, When::BothLoops, "Two loops play at once: {loop1} ({loop1_len}) and {loop2} ({loop2_len}). Their beat counts share no factor, so they drift in and out of step and only line up again after {loops_meet}, the same trick as the glitch layers' 7 and 11 steps."),
    (Loops, When::BothLoops, "Loop 2 sits narrower in the stereo field than loop 1 and swells on different slow cycles, so the two never breathe in at the same moment."),
    (Loops, When::Loops, "This version's atmosphere loops: {loops}. Each is a few seconds of sound, synthesised once at start-up for each chord and then looped for ever, the way 90s sample CDs and game soundtracks built their pads."),
    (Loops, When::Loops, "Loop lengths here: {loops_len}. Each is a whole number of beats, so it stays locked to the tempo. Three to eight seconds was the sweet spot of 90s samplers: long enough to breathe, short enough to fit in a few megabytes of memory."),
    (Loops, When::Loops, "The loop point is hidden with a crossfade: the sound just after the loop's end is faded into its first 150 ms, so the jump back to the start never clicks. Hardware samplers like the Akai S1000 had a 'loop crossfade' for exactly this."),
    (Loops, When::Loops, "Everything that moves inside the loop (the vowel, the wave steps, the swells, the strikes) is timed to repeat exactly once per loop. Each pass sounds the same, while the music around it keeps changing."),
    (Loops, When::Loops, "Each chord gets its own loop. During a chord's last bar the next chord's loop fades in at the same position (equal power), so the pad changes harmony without ever restarting."),
    (Loops, When::Loops, "Seed 6 chooses which chord tones each loop plays (moved into the octave above middle C, never closer than a whole tone) and the details of its movement."),
    (Loops, When::Loops, "The loops step back for the drums exactly like the drone does, and drift a little in level with two of the slow cycles, so even a repeating loop never sits quite still in the mix."),
    (Loops, When::Loop(LoopTimbre::Choir), "Choir \"Aah\": nine detuned sawtooth voices pass through three resonant band-pass filters set to vocal formants. Gliding the formants from 'ah' through 'oh', 'oo' and 'eh' is how the synth choirs of the Korg M1 and Roland JD-800 era sang."),
    (Loops, When::Loop(LoopTimbre::Glass), "Glass: FM with two modulators at inharmonic ratios (3.5 and 7.07), struck softly on beats and half-beats. Brightness fades faster than volume, so each strike rings out as a pure tone: the DX7's glassy bell family."),
    (Loops, When::Loop(LoopTimbre::Fantasia), "Fantasia: a bright FM bell strum over a warm, detuned saw pad, after the Roland D-50's famous 1987 preset, which layered a sampled attack over a synth pad. The idea defined the late-80s and 90s 'new age' pad."),
    (Loops, When::Loop(LoopTimbre::WaveSeq), "Wave sequence: on every beat the loop moves to a new single-cycle waveform, crossfading over the last quarter-beat. Korg's Wavestation (1990) made whole evolving, rhythmic pads this way."),
    (Loops, When::Loop(LoopTimbre::Breath), "Breath: white noise through very narrow band-pass filters tuned to the chord, plus a wide 'air' band. Each note swells in turn, like a breathy pan-flute or wind pad."),
    (Loops, When::Loop(LoopTimbre::Whisper), "Whisper: white noise is blown through three resonant band-passes tuned to the vowel formants ('ah', 'oh', 'oo', 'eh'), and the formants glide through them once per loop. It is the wordless, breathy pad behind a lot of Enya and Deep Forest records."),
    (Loops, When::Loop(LoopTimbre::Wind), "Wind: one low-pass gusting between roughly 250 Hz and 1.8 kHz, with a soft resonance on the lowest chord note. Slow filter sweeps like this were the New Age 'wind' pad: cheap to synthesise, endless to listen to."),
    (Loops, When::Loop(LoopTimbre::Shimmer), "Shimmer: narrow high resonances on the chord two octaves up, each tremolo-ing at its own slow rate, over a wide air band. The Ensoniq and Korg air textures of the early 90s were exactly this: high, glassy glints with no strong pitch."),
    (Loops, When::Loop(LoopTimbre::Stream), "Stream: very narrow bands on each chord note, each fluttering at its own fast whole-number rate (6 to 11 times per loop) over a 1.5 kHz band. Water over stones — the 'nature' wash of 90s ambient, made from noise instead of a recording."),
    (Loops, When::Loop(LoopTimbre::Aurora), "Aurora: four resonant bands sweep a whole octave up and back down once per loop, each at its own phase, brightening as they climb. A Wavestation or JD-800 'aurora' pad: a curtain of light that opens and closes over the chord."),
    (Loops, When::Loop(LoopTimbre::Haze), "Haze: a steep low-pass breathing between 200 and 800 Hz over one wide low resonance on the chord's root. It is almost pitchless — the quiet bed under a 90s ambient mix, like the lowest layer of an Aphex or FSOL soundscape."),
    (Cycles, When::DroneArc, "This version's drone breathes: {drone_arc}. The gain is a pure function of the sample clock, so the swell is identical every cycle and at every block size."),
    (Cycles, When::DroneArc, "The drone arc is measured in seconds, not beats, so changing the BPM moves the rhythm but leaves this slow entrance and exit alone — the arrangement trick behind a lot of 90s ambient, where a pad drifts in for a minute and then is gone."),
    (Cycles, When::DroneArc, "Inside the hold the drone fades up over a raised cosine, sits at full level, then fades back down: no clicks, and no need for the player to press anything. The bass follows the same envelope (it leaves with the drone), while the atmosphere loops keep playing underneath."),
    (Loops, When::EventLoops, "Physical textures are events, not pads: each marked cell on the 4 x 16 grid fires a short synthesised grain — a band-passed crackle, a struck resonance, a bending creak or a swell of flow. Fire, water, stones and wood are just starting grids; every cell is yours."),
    (Loops, When::EventLoops, "Nothing here is a sample pack. Crackles are noise through a narrow band-pass, knocks are an impulse into three inharmonic resonances tuned to a chord tone, creaks are noise through a bending filter, and hiss is a slowly moving band. The 'nature' is synthesised from first principles."),
    (Loops, When::EventLoops, "The grains are wrapped circularly into the loop, so a tail that would run past the end is already present at the start — the same trick a DAW uses when it loops a reverb tail or a foley recording."),
    (Loops, When::Loops, "Both atmosphere layers play the chord tones, so they move with the chosen root note and scale. A transpose at setup can shift them away from the chord (0 = exactly on it), which is how you get a drone that sits a fifth above or an octave below."),
    (Harmony, When::Always, "The root note comes from {key_source}. The scale supplies the intervals, so changing the root just moves the whole piece without changing its shape."),
    (General, When::NoLoops, "This version has no atmosphere loops (both Off). Choose a timbre at setup to add a 90s sample-CD pad that follows the chords."),
    // Harmony.
    (Harmony, When::Always, "The scale you choose decides which notes exist; seed 1 decides the key and how the chords are stacked from those notes. This version is in {key} {scale}."),
    (Harmony, When::Always, "This version cycles through {chords} over 32 bars, each chord a dense stack of {scale} notes."),
    (Harmony, When::Always, "Each chord is stacked from the scale in thirds (root, 3rd, 5th, 7th, 9th, 11th, 13th), with some tones left out and the rest spread over about three octaves. Every note is in the scale, but the stack is dense enough to sound suspended and unresolved."),
    (Harmony, When::Always, "Chord weights over a 32-bar cycle: one chord holds all 32 bars; two split 20/12; three split 16/12/4; four split 14/10/4/4. The first two chords do most of the work, and short later chords act as passing colours."),
    (Harmony, When::Always, "Chord pace sets how long a bar lasts. At half-time a bar is {bar_half} ({half} BPM), so a 16-bar chord takes 16 of those. At jungle pace a bar is {bar_jungle}, the drums' own bar at {bpm} BPM."),
    (Harmony, When::Always, "Later chords prefer strong scale degrees (the 4th, 5th and 6th) and never repeat the previous one, so the progression always moves but stays inside the scale's colour."),
    (Harmony, When::Always, "Lydian's raised 4th floats; Phrygian's flat 2nd darkens; whole-tone has no gravity at all; Hirajoshi and the pentatonic leave gaps so nothing can clash. The same seed in a different scale gives the same shape in a new colour."),
    (Spectrum, When::Always, "Chord changes are spectral morphs. During a chord's last bar, every new frame blends the two frozen spectra's power, bin by bin. Because the phases are random anyway, the drone dissolves from one chord into the next with no seam."),
    (Bass, When::Always, "The bass follows the progression: during each morph bar it glides to the new chord's root, so the low end moves with the harmony."),
    // The drone.
    (Chord, When::Always, "Each chord note is a 3-operator FM voice: one sine wave wobbles the phase of another, building rich overtones out of pure sines."),
    (Chord, When::Always, "FM ratio picks which overtones appear (whole numbers sound harmonic; 4.003 adds a slow shimmer). FM index sets how bright the tone is."),
    (Chord, When::Always, "Two FM wirings: a stack (3 modulates 2, which modulates 1) or a pair (2 and 3 both modulate 1). Operator 3 also feeds back into itself."),
    (Chord, When::Always, "Every chord is rendered only once, for 8 seconds, at start-up. You never hear them directly, only their frozen spectra."),
    (Spectrum, When::Always, "Freezing: each 8 s chord is cut into overlapping windows of 32,768 samples. Each is run through an FFT, and the magnitudes are averaged. Timing is thrown away; only how much of each frequency there was remains."),
    (Spectrum, When::Always, "An FFT (Fast Fourier Transform) turns a slice of sound into its frequencies: here 16,384 bins, each about 1.46 Hz wide."),
    (Spectrum, When::Always, "The freezing trick: keep every bin's magnitude, but give each bin a brand-new random phase in every frame. The timbre stays; the motion dissolves into an endless, still cloud."),
    (Spectrum, When::Always, "Left and right get independent random phases, so the drone is wide and can never cancel itself out in stereo."),
    (Spectrum, When::Always, "Both channels come from one complex inverse FFT: left rides in the real part, right in the imaginary part. Two transforms for the price of one."),
    (Spectrum, When::Always, "The bright region is the drone's low-pass filter sweeping across the frozen spectrum. Frequencies above the cutoff are dimmed."),
    (Spectrum, When::Always, "Bins below 110 Hz are emptied from the frozen spectra, leaving that range to the bass."),
    (Ola, When::Always, "Overlap-add: each new frame is shaped by a Hann window (a smooth hump) and overlaps the three before it by 75%."),
    (Ola, When::Always, "Why 75%? Four squared Hann windows, each a quarter apart, always add up to exactly 1.5, so random-phase frames blend at perfectly steady loudness."),
    (Ola, When::Always, "A new frame starts every 8,192 samples (171 ms). Each is a different random grain of the same frozen sound."),
    (Ola, When::Always, "To keep the browser version glitch-free, each frame's work is sliced into 25 small jobs (fill the spectrum, bit-reverse, 15 FFT stages, apply the window), one every 256 samples."),
    // Slow cycles.
    (Cycles, When::Always, "Seven slow LFOs (low-frequency oscillators) shape the form. Their periods are 41, 61, 97, 113, 151, 233 and 307 seconds, all prime numbers."),
    (Cycles, When::Always, "Because every period is prime, the combined pattern repeats only after their product: 296,097,754,097,441 seconds, about 9.4 million years."),
    (Cycles, When::Always, "LFO positions are calculated from the sample counter (clock mod period) instead of being added up step by step, so they never drift, however long the piece plays."),
    (Cycles, When::Always, "The LFOs mix into seven controls: drone filter openness and resonance, each glitch layer's density, bass brightness, bass swell, and the drums' energy."),
    // Glitches.
    (Glitch, When::Always, "Glitch layer 1 loops 7 sixteenth-note steps; layer 2 loops 11 eighth-note steps. 7 and 11 share no factors, so the layers drift in and out of phase."),
    (Glitch, When::Always, "Cyclic repeats: each layer plays its pattern 3 to 9 times, then mutates one step by replacing it, clearing it, re-pitching it, or rotating the whole loop."),
    (Glitch, When::Always, "Every step has a threshold. It sounds only while the layer's density (set by the slow LFOs) is above it, so patterns thin out and fill back in over minutes."),
    (Glitch, When::Always, "Ratchets: some steps fire 2 to 4 times within their slot for a stuttering roll."),
    (Glitch, When::Always, "B: Blip. An FM tone crushed to a few levels and sample-held: deliberately aliased digital grit."),
    (Glitch, When::Always, "S: Stutter. A tiny grain copied from the last 1.4 s of the drone and looped, sometimes backwards: the drone chopped into rhythm."),
    (Glitch, When::Always, "P: Ping. Two sines at a 1 : 2.756 ratio (the inharmonic partials of a small bell) with a long exponential decay."),
    (Glitch, When::Always, "T: Tick. White noise, differentiated (each sample minus the one before) so only its crackly top end remains."),
    (Glitch, When::Always, "C: Click. A few milliseconds of square-wave buzz, the sound of a digital dropout."),
    (Glitch, When::Always, "X: Crush. Noise held for several samples and quantised to three levels: a burst of broken data."),
    (Glitch, When::Always, "All glitch pitches come from the chosen scale in the chosen key, so even the noise belongs to the harmony."),
    // Jungle drums.
    (Drums, When::Drums, "This version's drums use the {kit} kit, shaped by seed 5. The jungle drums are fully synthesised, with no samples anywhere. You choose one of ten kit families at setup; seed 5 then sets every voice's numbers and the break within that family's ranges, so no two seeds sound quite alike."),
    (Drums, When::Kit(Kit::Acoustic), "Acoustic break kit: a sine kick whose pitch dives from around 170 Hz to 50 Hz, a snare with two tuned body modes and band-passed noise for the wires, and cymbals from clashing square waves. The closest to a sampled funk break."),
    (Drums, When::Kit(Kit::FmMetal), "FM metal kit: every drum is frequency modulation. The kick's modulation index falls to zero as it decays, the snare uses a clangy inharmonic ratio, and the hats are short FM bells."),
    (Drums, When::Drums, "Ghost notes are the same snare played quietly with short wires: the soft in-between hits that give a funk break its swing."),
    (Drums, When::Kit(Kit::Modal), "Modal kit: drums as sets of resonating modes. The kick's membrane modes are tuned to the key, and the hats and ride are struck bars tuned to notes of the scale, so the drums belong to the harmony."),
    (Break, When::Drums, "Jungle was built from sampled funk breaks. Here the break is first played by the synthesised kit at a funk-record tempo (125 to 150 BPM, set by the seed), with human timing: swing on the off-beats and a millisecond of push and pull."),
    (Break, When::HeavyKit, "Then it is 'sampled' through a sampler colour chain: saturation, sample-and-hold, bit reduction (8 to 14 bits, set by the kit) and a lowpass. That is the gritty colour of late-80s and early-90s samplers."),
    (Break, When::Drums, "The break is played back at {bpm} BPM, like changing the speed on a sampler, so its pitch moves with it. Faster than it was recorded (125 to 150 BPM) it rises: at 160 to 180 BPM that is the classic sped-up jungle sound. Slower, it drops and drags."),
    (Break, When::Drums, "Chopping: the two-bar break is cut into 32 sixteenth-note slices. Each bar plays 16 of them, in order unless an edit moves them."),
    (Break, When::Drums, "Edits: some slices are swapped for kick or snare slices from elsewhere, played backwards, rolled (retriggered as 1/32s), or pitched down. The more energetic the section, the more edits."),
    (Break, When::Drums, "Fills: the last bar of every 4-bar phrase gets extra edits and, often, a snare roll that throws the beat into the next phrase."),
    (Break, When::Drums, "Half-time sections play the break at half speed across two steps per slice: an octave lower and twice as heavy, the 'drag' of darker jungle."),
    (Drums, When::Kit(Kit::Crush), "Digital crush kit: crushed to 4 or 5 bits and sample-starved, with 1-bit pulse-train hats. Drums made of broken data."),
    (Drums, When::Kit(Kit::NoiseSculpt), "Noise sculpture kit: only filtered noise. The kick is a resonant low-pass sweeping down, the snare a band-pass sweep, the hats resonant pings."),
    (Drums, When::Kit(Kit::SubClicks), "Sub & clicks kit: short, deep sine kicks with a gentle glide, and snares and hats reduced to tiny clicks. Sparse and airy, almost ambient."),
    (Drums, When::LightKit, "The light kits (Sub & clicks, Micro blips, Glass pings, Data dust, Pulse code) borrow from the glitch layers' palette: tiny, quiet, digital hits with lots of silence between them, so the drums float with the drone instead of pushing against it."),
    (Drums, When::Kit(Kit::MicroBlips), "Micro blips kit: 50 to 80 ms sine thumps, snares that are crushed FM blips (a few quantisation levels, sample-held), and hats made of differentiated-noise ticks, the same recipe as the glitch layers' T."),
    (Drums, When::Kit(Kit::GlassPings), "Glass pings kit: soft round kicks with no click, and snares and hats that are little bells (two sines at 1 : 2.756) tuned to notes of the scale."),
    (Drums, When::Kit(Kit::DataDust), "Data dust kit: a short sine tock, snares made of sparse random impulses rung through a band-pass, like a burst of vinyl crackle, and single-sample dust for hats."),
    (Drums, When::Kit(Kit::PulseCode), "Pulse code kit: square-wave beeps for kicks, a two-tone dropout buzz for snares, and hats that are just one or two samples of opposite polarity: the smallest possible click."),
    (Break, When::LightKit, "Light kits use only a touch of the punch chain and a clean 14-bit sampler, so their hits stay delicate; the older, heavier kits get the full compression and grit."),
    (Drums, When::Drums, "Cymbal choke: in a real kit a hand stops a ringing cymbal. Here every hat and ride is cut short, with a 2 ms fade, the moment the next one is struck, so the top end stays crisp instead of washing out."),
    (Drums, When::Kit(Kit::Acoustic), "Acoustic hats vary from hit to hit: the metallic oscillators are detuned by up to 3%, the noise colour is re-rolled, and harder hits are brighter."),
    (Break, When::HeavyKit, "Punch: before sampling, each drum's first few milliseconds are boosted and driven, then the whole break goes through a 4:1 compressor and an asymmetric soft clip, like a break bussed through hardware. All of this happens once, at start-up."),
    (Spectrum, When::Drums, "Making room for the drums: while the break plays, the drone dips about 5 dB, pumps gently on each hit, and loses 6 dB above 2.5 kHz. When the drums drop out, the drone swells back to full."),
    (General, When::Always, "Every version has a recipe. This one is {recipe}: the track, scale, chord count and pace (H or J), the BPM, the drum kit and space, the two loop timbres, then the six seeds. It is shown in the header, saved in each WAV's metadata, and recreates the version with --recipe."),
    (General, When::Always, "The setup screen remembers your last answers as the next defaults, and r on a seed question rolls a random seed."),
    (Drums, When::Drums, "Arrangement: every 4-bar phrase picks a section (out, hats only, half-time, full, or full with rolls) from the slow drum-energy LFO, so the drums breathe with the ambience."),
    (Drums, When::Drums, "The drums stay out for the first two phrases, then enter on hats alone: the piece always opens as ambient music before the break arrives."),
    (Drums, When::Drums, "{bpm} BPM is exactly twice the glitch layers' {half} BPM, so a drum sixteenth ({drum16} samples) is half a glitch sixteenth ({glitch16}). The two rhythms lock together sample for sample."),
    (Drums, When::Drums, "Where slices join seamlessly, the break plays straight through. Where an edit jumps, a 0.5 ms fade hides the click, so the cuts sound sharp but never broken."),
    (General, When::NoDrums, "This version has no drums (kit: Off). The drone, glitches and bass carry it alone, and the drone is never ducked. Pick a kit at setup to add a break."),
    // Effects.
    (Echo, When::Always, "The echo is a ping-pong delay. Sound enters on the left, then bounces right, left, right every three glitch sixteenths ({echo_ms}): it moves with the tempo."),
    (Echo, When::Always, "Each bounce passes through a low-pass filter, so repeats grow darker as they fade, like tape or air."),
    (Reverb, When::Always, "The reverb is a feedback delay network: 8 delay lines of 63 to 169 ms, mixed by an 8x8 Hadamard matrix and fed back into themselves."),
    (Reverb, When::Always, "A Hadamard matrix holds only +1s and -1s. It mixes every line into every other without adding or losing energy, so the tail grows dense and smooth."),
    (Reverb, When::Always, "Each line's feedback gain makes the tail fall 60 dB in 18 seconds. A low-pass in every loop makes the highs die sooner, like a real room."),
    (Reverb, When::Always, "Before the reverb, four all-pass diffusers per side smear sharp transients, so glitches bloom instead of fluttering."),
    (Reverb, When::Always, "The reverb input is high-passed at 180 Hz, so 18 seconds of tail never build up into low-end mud."),
    (Reverb, When::Drums, "The drums send only a little to the reverb, so the break stays dry and close while everything else floats."),
    (Bass, When::Always, "The bass is a 2-operator FM tone plus a pure sine an octave below it, both on the current chord's root."),
    (Bass, When::Always, "The FM bass is detuned from the sub by up to 1.5 cents, so the two beat slowly against each other."),
    (Bass, When::Always, "When the bass-brightness LFO rises, it raises the FM index and opens the low-pass (140 to 500 Hz) together."),
    (Output, When::Always, "Master: a DC blocker removes any offset, then a tanh soft clipper rounds peaks off smoothly instead of clipping them."),
    (Output, When::Always, "The piece fades in over its first 10 seconds. Rendered files fade out over their last 40 seconds; live playback never ends."),
    (Output, When::Always, "What you see is in sync with what you hear. The engine runs about 170 ms ahead, and each snapshot waits until its samples reach the sound card."),
    // Under the hood.
    (General, When::Always, "The engine has zero dependencies. It has its own sine, exponential, tanh, random numbers and FFT, built only from + - x / and square roots."),
    (General, When::Always, "Why not use the system's sin()? Maths libraries round differently between operating systems and versions, enough to make two renders of the same track differ."),
    (General, When::Always, "The browser version is this same Rust code compiled to WebAssembly. A test hashes 30 s of output from both builds, and the hashes match bit for bit."),
    (General, When::Always, "Output is block-size independent: whether the sound card asks for 128 samples or 65,536 at a time, every sample comes out identical. Even each drum bar's plan is a pure function of the bar number."),
    (General, When::Always, "Everything runs at a fixed 48,000 samples per second, in 64-bit floating point, from a single integer sample clock."),
    (General, When::Always, "These visuals only read from the engine and never touch it. The golden-hash test proves the sound is identical with or without them."),
    (General, When::Always, "Random numbers come from SplitMix64, a tiny integer-only generator whose output is fully defined by its arithmetic, on every machine."),
    (Drums, When::Panned, "Every drum hit has its own place in the stereo field, rolled once from seed 5 when the break is built: the kick stays centred, snares and clicks sit 45–65% to one side, hats 50–75%."),
    (Drums, When::Panned, "Panning is baked into the break before it is sampled, so a chopped or reversed slice carries its hit's position with it."),
    (Drums, When::TapeEcho, "The snares feed a ping-pong tape echo at three 16ths of the drum tempo ({tape_ms}). The bounces go about halfway out to each side, its delay time wobbles slowly like worn tape, and each repeat is saturated and filtered, so the echoes grow darker and thinner."),
    (General, When::Always, "This list is shuffled each time you play: every insight appears once before any repeats, so even short sessions show a different selection."),
];

#[cfg(test)]
mod tests {
    use super::*;
    use shrine0011::kits::KITS;

    #[test]
    fn insights_only_describe_the_playing_kit() {
        for kit in KITS {
            for (_, when, text) in EXPLAINERS {
                if !when.applies(kit, DrumSpace::Tape, [LoopDesign::Sustained(LoopTimbre::Choir), LoopDesign::OFF], DroneArc::ALWAYS_ON) {
                    continue;
                }
                for other in KITS.iter().filter(|k| **k != kit && **k != Kit::Off) {
                    let tag = format!("{} kit", other.name());
                    assert!(!text.starts_with(&tag), "{tag} shown while playing {}", kit.name());
                }
                if kit == Kit::Off {
                    assert!(!matches!(when, When::Drums | When::Kit(_) | When::LightKit | When::HeavyKit | When::Panned | When::TapeEcho), "{text}");
                }
            }
        }
    }

    #[test]
    fn loop_insights_only_describe_the_playing_timbre() {
        use shrine0011::atmos::LOOP_TIMBRES;
        for t in LOOP_TIMBRES {
            for (_, when, text) in EXPLAINERS {
                if !when.applies(Kit::Off, DrumSpace::Centred, [LoopDesign::Sustained(t), LoopDesign::OFF], DroneArc::ALWAYS_ON) {
                    continue;
                }
                for other in LOOP_TIMBRES.iter().filter(|o| **o != t && **o != LoopTimbre::Off) {
                    assert!(!text.starts_with(&format!("{}:", other.name())), "{} shown while playing {}", other.name(), t.name());
                }
                if t == LoopTimbre::Off {
                    assert!(!matches!(when, When::Loops | When::Loop(_)), "{text}");
                }
            }
        }
    }

    #[test]
    fn placeholders_are_filled() {
        let v = Vars {
            kit: "K".into(),
            key: "D".into(),
            scale: "S".into(),
            chords: "3 chords".into(),
            recipe: "R".into(),
            tempo: Tempo::new(120),
            loop1: "L".into(),
            loop2: "M".into(),
            loop1_len: "8 beats".into(),
            loop2_len: "5 beats".into(),
            loops_meet: "40 beats".into(),
            loops: "loop 1 L".into(),
            loops_len: "loop 1 8 beats".into(),
            key_source: "seed 1".into(),
            drone_arc: "every 120 s the drone swells up for 40 s".into(),
            drone_cycle: "120".into(),
            drone_hold: "40".into(),
        };
        for (_, _, text) in EXPLAINERS {
            assert!(!fill(text, &v).contains('{'), "{text}");
        }
    }
}
