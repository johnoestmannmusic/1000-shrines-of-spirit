//! The Insights shown in the ticker. Each is tagged with the panel it
//! describes (that panel lights up while its insight is showing) and with
//! when it applies, so a version only ever shows insights that are true for
//! it. `{kit}`, `{key}`, `{scale}`, `{chords}` and `{recipe}` are filled in
//! with this version's values.

use super::Panel::{self, *};
use shrine0010::kits::Kit;

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
}

impl When {
    pub fn applies(self, kit: Kit) -> bool {
        let drums = kit != Kit::Off;
        match self {
            When::Always => true,
            When::Kit(k) => k == kit,
            When::Drums => drums,
            When::NoDrums => !drums,
            When::LightKit => kit.is_light(),
            When::HeavyKit => drums && !kit.is_light(),
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
}

pub fn fill(text: &str, v: &Vars) -> String {
    text.replace("{kit}", &v.kit)
        .replace("{key}", &v.key)
        .replace("{scale}", &v.scale)
        .replace("{chords}", &v.chords)
        .replace("{recipe}", &v.recipe)
}

pub const EXPLAINERS: &[(Panel, When, &str)] = &[
    // The piece as a whole.
    (General, When::Always, "0010 is not a recording. It is a program that composes and synthesises the music live, one sample at a time, forever."),
    (Flow, When::Always, "Press Tab to switch views: 1 pipeline (the sound travelling through each chain), 2 engine (the drone, glitches and effects opened up), 3 rhythm & harmony (chords and drums), 4 learn (how to make this yourself)."),
    (Flow, When::Always, "View 4, Learn, walks through how this piece is made, lesson by lesson, with this version's real settings, so you could rebuild it in your own studio. Use the up and down arrows there to choose a lesson."),
    (General, When::Always, "This version's recipe is {recipe}. Paste it into --recipe to hear exactly this again."),
    (General, When::Always, "Every sample is computed from five seeds, four choices (scale, chord count, chord pace, drum kit) and a counter. The same inputs give the exact same music on any computer, now or in 20 years."),
    (General, When::Always, "Seed 1 sets the key and builds the chords, seeds 2 and 3 shape the two glitch layers, seed 4 the bass, and seed 5 the chosen drum kit's sounds and break. Choosing the kit 'Off' leaves the drums out entirely."),
    // Harmony.
    (Harmony, When::Always, "The scale you choose decides which notes exist; seed 1 decides the key and how the chords are stacked from those notes. This version is in {key} {scale}."),
    (Harmony, When::Always, "This version cycles through {chords} over 32 bars, each chord a dense stack of {scale} notes."),
    (Harmony, When::Always, "Each chord is stacked from the scale in thirds (root, 3rd, 5th, 7th, 9th, 11th, 13th), with some tones left out and the rest spread over about three octaves. Every note is in the scale, but the stack is dense enough to sound suspended and unresolved."),
    (Harmony, When::Always, "Chord weights over a 32-bar cycle: one chord holds all 32 bars; two split 20/12; three split 16/12/4; four split 14/10/4/4. The first two chords do most of the work, and short later chords act as passing colours."),
    (Harmony, When::Always, "Chord pace sets how long a bar lasts. At half-time a bar is 2.86 s (84 BPM), so a 16-bar chord lasts about 46 s. At jungle pace a bar is 1.43 s, the drums' own bar."),
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
    (Break, When::Drums, "The break is played back sped up to 168 BPM, like turning up the pitch on a sampler. Depending on its original tempo it rises 2 to 5 semitones, the classic sped-up jungle sound."),
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
    (General, When::Always, "Every version has a recipe. This one is {recipe}: the track, scale, chord count and pace (H or J), the drum kit, then the five seeds. It is shown in the header, saved in each WAV's metadata, and recreates the version with --recipe."),
    (General, When::Always, "The setup screen remembers your last answers as the next defaults, and r on a seed question rolls a random seed."),
    (Drums, When::Drums, "Arrangement: every 4-bar phrase picks a section (out, hats only, half-time, full, or full with rolls) from the slow drum-energy LFO, so the drums breathe with the ambience."),
    (Drums, When::Drums, "The drums stay out for the first two phrases, then enter on hats alone: the piece always opens as ambient music before the break arrives."),
    (Drums, When::Drums, "168 BPM is exactly twice the glitch layers' 84 BPM, so a drum sixteenth (4,284 samples) is half a glitch sixteenth (8,568). The two rhythms lock together sample for sample."),
    (Drums, When::Drums, "Where slices join seamlessly, the break plays straight through. Where an edit jumps, a 0.5 ms fade hides the click, so the cuts sound sharp but never broken."),
    (General, When::NoDrums, "This version has no drums (kit: Off). The drone, glitches and bass carry it alone, and the drone is never ducked. Pick a kit at setup to add a break."),
    // Effects.
    (Echo, When::Always, "The echo is a ping-pong delay. Sound enters on the left, then bounces right, left, right every 3/16 of a bar (535 ms)."),
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
    (General, When::Always, "This list is shuffled each time you play: every insight appears once before any repeats, so even short sessions show a different selection."),
];

#[cfg(test)]
mod tests {
    use super::*;
    use shrine0010::kits::KITS;

    #[test]
    fn insights_only_describe_the_playing_kit() {
        for kit in KITS {
            for (_, when, text) in EXPLAINERS {
                if !when.applies(kit) {
                    continue;
                }
                for other in KITS.iter().filter(|k| **k != kit && **k != Kit::Off) {
                    let tag = format!("{} kit", other.name());
                    assert!(!text.starts_with(&tag), "{tag} shown while playing {}", kit.name());
                }
                if kit == Kit::Off {
                    assert!(!matches!(when, When::Drums | When::Kit(_) | When::LightKit | When::HeavyKit), "{text}");
                }
            }
        }
    }

    #[test]
    fn placeholders_are_filled() {
        let v = Vars { kit: "K".into(), key: "D".into(), scale: "S".into(), chords: "3 chords".into(), recipe: "R".into() };
        for (_, _, text) in EXPLAINERS {
            assert!(!fill(text, &v).contains('{'), "{text}");
        }
    }
}
