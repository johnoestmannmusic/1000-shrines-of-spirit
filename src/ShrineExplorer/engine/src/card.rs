//! The recipe card: everything needed to recreate a shrine, as plain text.
//! The artifact bakes the home's; the explorer asks for the card of
//! whichever shrine is showing.

use crate::sim::SimConfig;
use crate::track::LoopDesign;

/// The card for `config`, built on `build` (YYYYMMDD), standing at `map` on
/// its home's map.
pub fn card(config: &SimConfig, build: u32, map: (i32, i32)) -> String {
    let mut config = config.clone();
    config.settings.map = map;
    info(&config, build)
}

pub fn info(config: &SimConfig, build: u32) -> String {
    let s = &config.settings;
    let key = match s.key {
        crate::track::KeyChoice::Seed => "seed picks".to_string(),
        crate::track::KeyChoice::Note(n) => crate::harmony::key_name(n).to_string(),
    };
    let fx = if !s.master.enabled {
        "off".to_string()
    } else {
        let on: Vec<&str> = s.master.labels().iter().filter(|(_, on)| *on).map(|(name, _)| *name).collect();
        if on.is_empty() { "on · none".to_string() } else { on.join(", ") }
    };
    let mut lines = vec![
        format!("ShrineGen  v{build}"),
        String::new(),
        format!("seed       {}", config.seed),
        format!("biome      {}", config.biome.name()),
        format!("weather    {}", config.weather.name()),
        format!(
            "wildlife   {}",
            if config.settings.wildlife {
                crate::wildlife::Wildlife::new(config.seed, config.biome).summary()
            } else {
                "off".to_string()
            }
        ),
        String::new(),
        format!("scale      {}", s.scale.name()),
        format!("root note  {key}"),
        format!("chords     {}", s.chords),
        format!(
            "mode       {}",
            match s.pace {
                crate::harmony::Pace::Garage => {
                    format!("{} · {} · key lift {}", s.pace.name(), s.garage.progression_name(), s.garage.lift.name())
                }
                crate::harmony::Pace::Micro => format!(
                    "{} · pulse {} · grains {} · pips {} · dropouts {} · chords {} · drone shift {}",
                    s.pace.name(),
                    s.micro.pulse.name(),
                    s.micro.grains.name(),
                    s.micro.pips.name(),
                    s.micro.dropouts.name(),
                    s.micro.chords.name(),
                    s.micro.shift.name()
                ),
                _ => s.pace.name().to_string(),
            }
        ),
        format!("tempo      {} BPM", s.bpm),
        format!("start      {}", if s.start_s == 0 { "at the beginning".to_string() } else { format!("{} s in", s.start_s) }),
        format!("map        {}", if s.map == (0, 0) { "home · 0, 0".to_string() } else { format!("{}, {} from home", s.map.0, s.map.1) }),
        format!("kit        {}", s.kit.name()),
        format!("space      {}", s.space.name()),
        format!("drum rev   {}", if s.drum_reverse { "on" } else { "off" }),
        format!("master fx  {fx}"),
        format!("code over  {}", s.code.summary()),
        format!("arch       {}", s.arch.summary()),
        format!(
            "camera     {}",
            if s.camera_speed == 0 { "still".to_string() } else { format!("{:+}°/s", s.camera_speed) }
        ),
        format!("glitch 1   {}", s.glitch[0].summary()),
        format!("glitch 2   {}", s.glitch[1].summary()),
        format!("transpose  {} semitones", s.loop_transpose),
        String::new(),
        format!("drone arc  {}", arc(s.drone)),
        format!("drone tone {}", tone(s.drone_tone)),
    ];
    for i in 0..4 {
        lines.push(format!("layer {}    {}", i + 1, layer(s.loops[i])));
        lines.push(format!("  arc {}    {}", i + 1, arc(s.loop_arcs[i])));
    }
    lines.push(String::new());
    lines.push(format!(
        "seeds      {} {} {} {} {} {} {}",
        config.seeds.s1,
        config.seeds.s2,
        config.seeds.s3,
        config.seeds.s4,
        config.seeds.s5,
        config.seeds.s6,
        config.seeds.s7
    ));
    lines.push("           keys&chords · glitch1 · glitch2 · bass · drums · atmosphere · drone rhythm".to_string());
    lines.push(format!(
        "rhythm     drone {} · loops {}",
        if config.settings.drone_rhythm { "on" } else { "off" },
        if config.settings.loop_rhythm { "on" } else { "off" }
    ));
    lines.push(String::new());
    lines.push("Music License: CC0 / Public Domain".to_string());
    lines.push("Code License: MIT".to_string());
    lines.join("\n")
}

fn arc(a: crate::track::DroneArc) -> String {
    if a.cycle_s == 0 {
        "always on".to_string()
    } else {
        format!("{}s cycle, {}s up", a.cycle_s, a.hold_s)
    }
}

fn tone(hz: u16) -> String {
    if hz == 0 {
        "off".to_string()
    } else {
        format!("{hz} Hz low-pass")
    }
}

fn layer(d: LoopDesign) -> String {
    match d {
        LoopDesign::Sustained(t) => t.name().to_string(),
        LoopDesign::Events(_) => "event grid".to_string(),
    }
}

