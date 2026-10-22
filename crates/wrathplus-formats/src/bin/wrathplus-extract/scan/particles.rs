//! Corpus scans over particle and ribbon emitters: the features the data authors, and the shapes
//! a consumer can get wrong.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use wrathplus_formats::Chain;

/// List the particle emitters whose file flags carry any bit of `mask`.
pub fn partscan(chain: &mut Chain, mask: u32, prefix: Option<&str>) -> Result<()> {
    let names = super::m2_names(chain, prefix)?;
    let (mut scanned, mut hits, mut emitters) = (0u32, 0u32, 0u32);
    for name in names {
        let Ok(bytes) = chain.read_file(&name) else {
            continue;
        };
        scanned += 1;
        let Ok(defs) = wrathplus_formats::parse_m2_particle_emitters(&bytes) else {
            continue;
        };
        let mut hit = false;
        for (i, d) in defs.iter().enumerate() {
            if d.flags & mask != 0 {
                hit = true;
                emitters += 1;
                println!(
                    "e{i} flags {:#010x}  {:?} {}  {name}",
                    d.flags,
                    d.shape,
                    match d.head_tail {
                        0 => "head",
                        1 => "tail",
                        _ => "head+tail",
                    },
                );
            }
        }
        if hit {
            hits += 1;
        }
    }
    eprintln!(
        "{scanned} models scanned, {hits} with mask {mask:#x} emitters ({emitters} emitters)"
    );
    Ok(())
}

/// Raw fields of one emitter record (stride `0x1f8`): the geometry-model (`+0x18`) and
/// recursion-model (`+0x20`) path `M2Array`s, and the emission-rate track's interpolation word
/// (`+0xdc`, 0 = step) and key count.
struct RecordExtras {
    geometry_model: Option<String>,
    recursion_model: Option<String>,
    rate_interp: u16,
    rate_keys: u32,
}

fn record_extras(bytes: &[u8]) -> Vec<RecordExtras> {
    const STRIDE: usize = 0x1f8;
    let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let str_at = |count: usize, ofs: usize| -> Option<String> {
        if count == 0 || ofs == 0 || ofs + count > bytes.len() {
            return None;
        }
        let s = String::from_utf8_lossy(&bytes[ofs..ofs + count])
            .trim_end_matches('\0')
            .to_string();
        (!s.is_empty()).then_some(s)
    };
    if bytes.len() < 0x144 || &bytes[..4] != b"MD20" {
        return Vec::new();
    }
    let count = u32_at(0x13c) as usize;
    let base = u32_at(0x140) as usize;
    if count == 0 || count > 256 || base + count * STRIDE > bytes.len() {
        return Vec::new();
    }
    (0..count)
        .map(|i| {
            let e = base + i * STRIDE;
            RecordExtras {
                geometry_model: str_at(u32_at(e + 0x18) as usize, u32_at(e + 0x1c) as usize),
                recursion_model: str_at(u32_at(e + 0x20) as usize, u32_at(e + 0x24) as usize),
                rate_interp: u16_at(e + 0xdc),
                rate_keys: u32_at(e + 0xdc + 0x14),
            }
        })
        .collect()
}

/// model key → the spells whose visual chain plays that model (any kit stage, or the missile).
fn spell_attribution(chain: &mut Chain) -> HashMap<String, Vec<(u32, String)>> {
    let (Ok(spells), Ok(visuals)) = (
        wrathplus_formats::load_spell_catalog(chain),
        wrathplus_formats::load_spell_visual_catalog(chain),
    ) else {
        eprintln!("(spell/visual DBCs unavailable — census runs without spell attribution)");
        return HashMap::new();
    };
    let mut map: HashMap<String, Vec<(u32, String)>> = HashMap::new();
    for (id, sp) in spells.iter() {
        let Some(st) = visuals.stages(sp.visual) else {
            continue;
        };
        let mut push = |effect: u32| {
            if effect != 0 {
                if let Some(p) = visuals.effect_path(effect) {
                    map.entry(crate::model_key(p))
                        .or_default()
                        .push((id, sp.name.clone()));
                }
            }
        };
        for kit_id in [st.precast, st.cast, st.impact, st.state, st.channel] {
            if kit_id == 0 {
                continue;
            }
            if let Some(kit) = visuals.kit(kit_id) {
                for (_, effect) in kit.effects() {
                    push(effect);
                }
            }
        }
        push(st.missile_model);
    }
    map
}

/// One census dimension: `model_count` counts every model, `models` keeps the first 64, sorted.
#[derive(Default)]
struct Tally {
    emitters: u32,
    model_count: u32,
    last_model: String,
    models: std::collections::BTreeSet<String>,
}

impl Tally {
    /// The walk visits each model's emitters consecutively, so "new model" is a change of name.
    fn hit(&mut self, model: &str) {
        self.emitters += 1;
        if self.last_model != model {
            self.model_count += 1;
            self.last_model = model.to_string();
        }
        if self.models.len() < 64 {
            self.models.insert(model.to_string());
        }
    }
}

/// Census which particle-emitter features the corpus authors, with the spells behind rare ones.
pub fn partcensus(chain: &mut Chain, prefix: Option<&str>) -> Result<()> {
    let attribution = spell_attribution(chain);
    let names = super::m2_names(chain, prefix)?;

    let mut tallies: std::collections::BTreeMap<&'static str, Tally> =
        std::collections::BTreeMap::new();
    let (mut scanned, mut with_emitters, mut total_emitters) = (0u32, 0u32, 0u32);
    for name in names {
        let Ok(bytes) = chain.read_file(&name) else {
            continue;
        };
        scanned += 1;
        let Ok(defs) = wrathplus_formats::parse_m2_particle_emitters(&bytes) else {
            continue;
        };
        if defs.is_empty() {
            continue;
        }
        with_emitters += 1;
        let extras = record_extras(&bytes);
        let key = crate::model_key(&name);
        for (i, d) in defs.iter().enumerate() {
            total_emitters += 1;
            let mut hit = |k: &'static str| tallies.entry(k).or_default().hit(&key);
            match d.shape {
                wrathplus_formats::ParticleShape::Plane => hit("shape:plane"),
                wrathplus_formats::ParticleShape::Sphere => hit("shape:sphere"),
                wrathplus_formats::ParticleShape::Spline => hit("shape:SPLINE"),
            }
            // Anisotropic plane areas, the only ones where the areaLength/areaWidth axis pairing
            // shows (a square renders the same either way), bucketed by aspect.
            let now = d.params.sample(None, 0.0, 0.0);
            if d.shape == wrathplus_formats::ParticleShape::Plane {
                let lo = now.area_length.abs().min(now.area_width.abs());
                let hi = now.area_length.abs().max(now.area_width.abs());
                if hi > 1e-4 {
                    let aspect = if lo > 1e-4 { hi / lo } else { f32::INFINITY };
                    if aspect >= 4.0 {
                        hit("plane-area:ANISOTROPIC >=4:1");
                    } else if aspect >= 1.5 {
                        hit("plane-area:anisotropic >=1.5:1");
                    }
                }
            }
            // Parameter channels that animate, per channel (Frost Nova's emission radius rides
            // its ring out).
            const PARAM_KEYS: [&str; 9] = [
                "param-anim:speed",
                "param-anim:speedVar",
                "param-anim:latitude",
                "param-anim:longitude",
                "param-anim:gravity",
                "param-anim:lifespan",
                "param-anim:areaLength",
                "param-anim:areaWidth",
                "param-anim:zSource",
            ];
            for (c, (_, slots)) in d.params.channel_views().iter().enumerate() {
                if slots
                    .iter()
                    .any(|k| k.is_some_and(|k| k.len() > 1 && k.iter().any(|&(_, v)| v != k[0].1)))
                {
                    hit(PARAM_KEYS[c]);
                }
            }
            match d.head_tail {
                0 => hit("type:head"),
                1 => hit("type:tail"),
                _ => hit("type:head+tail"),
            }
            match d.blend {
                wrathplus_formats::ParticleBlend::Add => hit("blend:add"),
                wrathplus_formats::ParticleBlend::Alpha => hit("blend:alpha"),
                wrathplus_formats::ParticleBlend::AlphaKey => hit("blend:alphakey"),
                wrathplus_formats::ParticleBlend::Opaque => hit("blend:opaque"),
                wrathplus_formats::ParticleBlend::Mod => hit("blend:mod"),
                wrathplus_formats::ParticleBlend::Mod2x => hit("blend:mod2x"),
            }
            if d.flags & 0x1 == 0 {
                hit("flag:LIT(0x1 clear)");
            }
            for (bit, label) in [
                (0x8u32, "flag:0x8 texenv"),
                (0x10, "flag:0x10 model-space"),
                (0x20, "flag:0x20 scale-by-instance"),
                (0x40, "flag:0x40 MOTION-VEL-INHERIT"),
                (0x80, "flag:0x80 KILL-OUTBOUND"),
                (0x100, "flag:0x100 sphere-up"),
                (0x200, "flag:0x200"),
                (0x400, "flag:0x400 tail-age-clamp"),
                (0x800, "flag:0x800 SPAWN-PATH-SPREAD"),
                (0x1000, "flag:0x1000 xy-quad"),
                (0x2000, "flag:0x2000 GROUND-SNAP"),
                (0x4000, "flag:0x4000 FOLLOW-DELTA"),
                (0x8000, "flag:0x8000 burst"),
            ] {
                if d.flags & bit != 0 {
                    hit(label);
                }
            }
            if d.flags >> 16 != 0 {
                hit("flag:HIGH-BITS(>0xffff)");
            }
            if d.spin > 0.0 {
                hit("spin:positive");
            }
            if d.spin < 0.0 {
                hit("spin:negative");
            }
            if d.twinkle_percent < 1.0 {
                hit("twinkle:percent<1");
            }
            if u32::from(d.tile_cols) * u32::from(d.tile_rows) > 1 {
                hit("tiles:atlas");
                if !d.tile_cols.is_power_of_two() {
                    hit("tiles:NON-POW2-COLS");
                }
            }
            if now.z_source != 0.0 {
                hit("kernel:zSource");
            }
            if now.gravity < 0.0 {
                hit("kernel:negative-gravity");
            }
            if d.shape == wrathplus_formats::ParticleShape::Sphere
                && now.vertical_range > 3.0
                && now.horizontal_range == 0.0
            {
                hit("kernel:edge-on-ring(lat±π,lon0)");
            }
            if let Some(x) = extras.get(i) {
                if x.geometry_model.is_some() {
                    hit("MODEL-PARTICLES(geometry)");
                }
                if x.recursion_model.is_some() {
                    hit("CHILD-EMITTERS(recursion)");
                }
                if x.rate_keys > 1 && x.rate_interp != 0 {
                    hit("rate:LERP-RAMP(interp!=0)");
                    // A burst emitter with a lerped rate would arm a near-zero count at its
                    // rising edge.
                    if d.burst() {
                        hit("rate:BURST+LERP(suspect)");
                    }
                }
            }
        }
    }

    println!(
        "== particle feature census  prefix={}  ({scanned} models scanned, {with_emitters} with emitters, {total_emitters} emitters)",
        prefix.unwrap_or("<all>"),
    );
    for (k, t) in &tallies {
        let ex: Vec<&str> = t.models.iter().take(3).map(|s| s.as_str()).collect();
        println!(
            "{:>6} emitters  {:>4} models  {k}  e.g. {}",
            t.emitters,
            t.model_count,
            ex.join(" · ")
        );
    }

    let detail: &[&str] = &[
        "MODEL-PARTICLES(geometry)",
        "CHILD-EMITTERS(recursion)",
        "shape:SPLINE",
        "blend:MOD(5)",
        "blend:MOD2X(6)",
        "flag:LIT(0x1 clear)",
        "flag:0x40 MOTION-VEL-INHERIT",
        "flag:0x80 KILL-OUTBOUND",
        "flag:0x800 SPAWN-PATH-SPREAD",
        "flag:0x2000 GROUND-SNAP",
        "flag:0x4000 FOLLOW-DELTA",
        "flag:HIGH-BITS(>0xffff)",
        "tiles:NON-POW2-COLS",
        "rate:LERP-RAMP(interp!=0)",
    ];
    for k in detail {
        let Some(t) = tallies.get(k) else { continue };
        println!();
        println!(
            "=== {k}  ({} emitters, {} models)",
            t.emitters, t.model_count
        );
        for m in t.models.iter().take(16) {
            let spells = attribution.get(m).map_or(String::new(), |v| {
                let mut seen = HashSet::new();
                let names: Vec<String> = v
                    .iter()
                    .filter(|(id, _)| seen.insert(*id))
                    .take(3)
                    .map(|(id, n)| format!("{id} {n}"))
                    .collect();
                if names.is_empty() {
                    String::new()
                } else {
                    format!(
                        "   [{}{}]",
                        names.join(", "),
                        if v.len() > 3 { ", …" } else { "" }
                    )
                }
            });
            println!("  {m}{spells}");
        }
        if t.model_count as usize > 16 {
            println!("  … {} more", t.model_count as usize - 16);
        }
    }
    Ok(())
}

/// Census the emitters' over-life flipbook fields: `INVERTED` (`begin > end`) plays the sheet
/// backwards, which a `clamp` into it panics on; `TAIL-RAMP`, a tail ramp (`+0x174`) unlike the
/// head's (`+0x168`); `PAST-ATLAS`, a cell past `rows·cols`, which the reference wraps to row 0
/// (it masks the column only); `REPEAT`, a segment repeat other than 1; `NON-POW2` and `MID`,
/// shapes the reference degrades on (a 1×1 fallback, a NaN sampler).
pub fn cellscan(chain: &mut Chain) -> Result<()> {
    let names = super::m2_names(chain, None)?;

    let (mut models, mut emitters) = (0u32, 0u32);
    let (mut inverted, mut tail_ramp, mut repeat_ne1) = (0u32, 0u32, 0u32);
    let (mut past_atlas, mut past_atlas_real, mut bad_tiles, mut bad_mid) =
        (0u32, 0u32, 0u32, 0u32);

    for name in names {
        let Ok(bytes) = chain.read_file(&name) else {
            continue;
        };
        models += 1;
        let Ok(defs) = wrathplus_formats::parse_m2_particle_emitters(&bytes) else {
            continue;
        };
        for (i, e) in defs.iter().enumerate() {
            emitters += 1;
            let ol = &e.over_life;
            let at = format!("{name} [{i}] {}x{}", e.tile_rows, e.tile_cols);
            let ramps = [
                ol.head_cells[0],
                ol.head_cells[1],
                ol.tail_cells[0],
                ol.tail_cells[1],
            ];
            let pair = |r: &wrathplus_formats::CellRamp| (r.begin, r.end);

            if ramps.iter().any(|r| r.begin > r.end) {
                inverted += 1;
                println!(
                    "INVERTED   {at} head {:?}/{:?}",
                    pair(&ol.head_cells[0]),
                    pair(&ol.head_cells[1])
                );
            }
            // Only a tail-drawing emitter (particleType 1 or 2) shows a tail ramp.
            if e.head_tail >= 1 && ol.tail_cells != ol.head_cells {
                tail_ramp += 1;
                println!(
                    "TAIL-RAMP  {at} head {:?}/{:?} tail {:?}/{:?}",
                    pair(&ol.head_cells[0]),
                    pair(&ol.head_cells[1]),
                    pair(&ol.tail_cells[0]),
                    pair(&ol.tail_cells[1])
                );
            }
            if ol.repeat.iter().any(|&r| r != 1.0) {
                repeat_ne1 += 1;
                println!("REPEAT     {at} {:?}", ol.repeat);
            }
            let atlas = e.tile_rows * e.tile_cols;
            if ramps.iter().any(|r| r.begin >= atlas || r.end >= atlas) {
                past_atlas += 1;
                // On a 1×1 sheet every index is the same texture: only a real atlas shows the wrap.
                if atlas > 1 {
                    past_atlas_real += 1;
                    println!(
                        "PAST-ATLAS {at} head {:?}/{:?}",
                        pair(&ol.head_cells[0]),
                        pair(&ol.head_cells[1])
                    );
                }
            }
            if !e.tile_rows.is_power_of_two() || !e.tile_cols.is_power_of_two() {
                bad_tiles += 1;
                println!("NON-POW2   {at}");
            }
            if !(ol.mid > 0.0 && ol.mid < 1.0) {
                bad_mid += 1;
                println!("MID        {at} mid {}", ol.mid);
            }
        }
    }

    eprintln!(
        "{models} models / {emitters} emitters scanned\n  \
         INVERTED    {inverted}\n  \
         TAIL-RAMP   {tail_ramp}\n  \
         REPEAT      {repeat_ne1}\n  \
         PAST-ATLAS  {past_atlas} ({past_atlas_real} on a real >1x1 atlas)\n  \
         NON-POW2    {bad_tiles}\n  \
         MID 0 or 1  {bad_mid}"
    );
    Ok(())
}

/// List the emitters whose file slot 0 never emits while another slot does (`peak0 <= 0 <
/// peakN`, the peak rates). The reference samples the playing sequence's rate window every frame
/// (`0x71850d`), so a consumer pinned to slot 0 never births a particle from them. `slots` counts
/// file sequence slots, the axis `EmitTiming` bakes on.
pub fn partslotscan(chain: &mut Chain, prefix: Option<&str>) -> Result<()> {
    let names = super::m2_names(chain, prefix)?;
    let (mut scanned, mut with_emitters, mut emitters, mut dead0) = (0u32, 0u32, 0u32, 0u32);
    let mut models = 0u32;
    for name in names {
        let Ok(bytes) = chain.read_file(&name) else {
            continue;
        };
        scanned += 1;
        let Ok(defs) = wrathplus_formats::parse_m2_particle_emitters(&bytes) else {
            continue;
        };
        if defs.is_empty() {
            continue;
        }
        with_emitters += 1;
        let mut lines = Vec::new();
        for (i, d) in defs.iter().enumerate() {
            emitters += 1;
            let views = d.timing.slot_views();
            if views.len() < 2 {
                continue; // one slot ⇒ nothing for a slot pick to get wrong
            }
            // Peak of a slot's baked rate keys; an unkeyed slot emits nothing (`None` ⇒ rate 0).
            let peak = |s: usize| -> f32 {
                views
                    .get(s)
                    .and_then(|v| v.1)
                    .map(|keys| keys.iter().map(|&(_, v)| v).fold(0.0, f32::max))
                    .unwrap_or(0.0)
            };
            let peak0 = peak(0);
            let (mut best, mut best_slot) = (0.0f32, 0usize);
            for s in 1..views.len() {
                if peak(s) > best {
                    best = peak(s);
                    best_slot = s;
                }
            }
            if peak0 <= 0.0 && best > 0.0 {
                dead0 += 1;
                lines.push(format!(
                    "    emitter {i:>2}: slots {:>2}  peak0 {peak0:>7.1}  peakN {best:>7.1} \
                     @ slot {best_slot}  tex {}",
                    views.len(),
                    d.texture.as_deref().unwrap_or("NONE"),
                ));
            }
        }
        if !lines.is_empty() {
            models += 1;
            println!("{name}");
            for l in lines {
                println!("{l}");
            }
        }
    }
    eprintln!(
        "{scanned} models scanned, {with_emitters} with emitters, {emitters} emitter(s): \
         {dead0} DEAD-IN-SLOT-0 across {models} model(s) — these emit nothing at all under a \
         pinned-slot-0 consumer"
    );
    Ok(())
}
