//! The keyed M2Track sampler shared by the particle and ribbon readers: one lane per sequence
//! slot, keys in sequence-local ms, plus an optional global-sequence clock.

/// A value a track can hold and lerp: `f32` scalars and the ribbon colour track's `[f32; 3]`.
pub trait TrackValue: Copy {
    /// The empty-track value; the parsers fall back to a constant key instead.
    const ZERO: Self;
    fn lerp(a: Self, b: Self, t: f32) -> Self;
}

impl TrackValue for f32 {
    const ZERO: Self = 0.0;
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        a + (b - a) * t
    }
}

impl TrackValue for [f32; 3] {
    const ZERO: Self = [0.0; 3];
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
    }
}

/// A keyed M2Track as `(timestamp ms, value)` pairs. Burst emitters key their rate
/// (`BloodSpurt.m2` goes `0 → 200 → 0` over 133 ms), so the first key alone reads 0. No loop
/// wrap: the last key holds, and every looping ambient prop in the shipped corpus keys a constant.
#[derive(Debug, Clone, Default)]
pub struct ValueTrack<V = f32> {
    /// `(timestamp ms, value)` keys, in file order (ascending timestamps).
    pub keys: Vec<(u32, V)>,
    /// Interpolation word (`+0x00`): 0 steps (`values[k0]`), as burst emitters author, else lerps.
    pub interp: u16,
}

impl<V: TrackValue> ValueTrack<V> {
    /// The first key's value, for constant-track parameters and diagnostics.
    pub fn first(&self) -> V {
        self.keys.first().map_or(V::ZERO, |&(_, v)| v)
    }

    /// Step-sample at `ms` since clip start, the reference sampler's interp-0 leg: the
    /// nearest-previous key, the first before the first, the last held past the end.
    pub fn step_ms(&self, ms: f32) -> V {
        let mut v = self.keys.first().map_or(V::ZERO, |&(_, v)| v);
        for &(t, val) in &self.keys {
            if (t as f32) <= ms {
                v = val;
            } else {
                break;
            }
        }
        v
    }

    /// Sample as the reference's per-frame sampler does (`0x71af20` → `0x713d50`, lerp at
    /// `0x71af76`): step at interp 0, else lerp, held past the last key and extrapolated backward
    /// below the first. Raw values may go negative; the consumer floors at 0 (`SetEmissionRate`).
    pub fn sampled_ms(&self, ms: f32) -> V {
        if self.interp == 0 {
            return self.step_ms(ms);
        }
        let n = self.keys.len();
        if n <= 1 {
            return self.first();
        }
        // The segment starting at the last key at or before `ms`; below the first key, the first.
        let k = self
            .keys
            .iter()
            .rposition(|&(t, _)| (t as f32) <= ms)
            .unwrap_or(0);
        if k + 1 == n {
            return self.keys[n - 1].1;
        }
        let (t0, v0) = self.keys[k];
        let (t1, v1) = self.keys[k + 1];
        let span = (t1.saturating_sub(t0)).max(1) as f32;
        V::lerp(v0, v1, (ms - t0 as f32) / span)
    }

    /// Sample at `ms` since clip start: linear between keys, clamped outside them; values are raw.
    pub fn sample_ms(&self, ms: f32) -> V {
        let Some(&(t0, v0)) = self.keys.first() else {
            return V::ZERO;
        };
        if ms <= t0 as f32 {
            return v0;
        }
        for w in self.keys.windows(2) {
            let ((ta, va), (tb, vb)) = (w[0], w[1]);
            if ms < tb as f32 {
                let span = (tb - ta).max(1) as f32;
                return V::lerp(va, vb, (ms - ta as f32) / span);
            }
        }
        self.keys.last().map_or(V::ZERO, |&(_, v)| v)
    }
}

impl ValueTrack<f32> {
    /// The track's peak, whether it can ever contribute (a burst's first key is 0, its peak not).
    pub fn peak(&self) -> f32 {
        self.keys.iter().fold(f32::MIN, |m, &(_, v)| m.max(v))
    }
}

/// A model's sequences in file order as `(AnimationData id, duration ms)`: the slot clocks
/// [`SeqTrack`] lanes key on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeqTable(pub Vec<(u16, u32)>);

impl SeqTable {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The file slot a fixed `AnimationData` id plays: its first sequence, else slot 0.
    pub fn slot_for(&self, anim: u16) -> usize {
        self.0.iter().position(|&(id, _)| id == anim).unwrap_or(0)
    }

    /// `ms` into `slot`, wrapped by the sequence's duration: a fixed-sequence owner loops it.
    pub fn wrap_ms(&self, slot: usize, ms: f32) -> f32 {
        match self.0.get(slot) {
            Some(&(_, d)) if d > 0 => ms.rem_euclid(d as f32),
            _ => ms,
        }
    }
}

/// Where a model's effect tracks are sampled this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeqClock {
    /// The owner's playing sequence slot (file order).
    pub slot: usize,
    /// Milliseconds into that sequence.
    pub seq_ms: f32,
    /// The scene clock in milliseconds, which global-sequence tracks loop on.
    pub scene_ms: f64,
}

/// A 3.3.5 M2Track on its native clock, as the animate pass samples ribbon and light state
/// (`0x82f0f0` → `0x82b0a0`/`0x82b340`/`0x82af40`): one key lane per sequence slot in
/// sequence-local time, the bone's slot falling back to slot 0 past the track's sequence count;
/// or, with a global sequence, one lane looping on the scene clock modulo its period. A slot
/// with no keys reads the caller's default, never a held value.
///
/// Unmodelled: the sampler's cross-fade between the outgoing and incoming sequence (bone state
/// `+0xa8`), and per-bone sequence state (this reads the owner model's playing slot).
#[derive(Debug, Clone)]
pub struct SeqTrack<V = f32> {
    /// `Some(period ms)` for a global-sequence track, whose keys are `slots[0]`.
    pub global_period_ms: Option<u32>,
    /// Keys per sequence slot in sequence-local ms.
    pub slots: Vec<ValueTrack<V>>,
    /// What a keyed track reads in a slot with no keys (the sampler's caller default).
    pub default: V,
    /// What a track with no keys anywhere reads: the animate pass never samples it, so the
    /// consumer keeps its load value.
    pub unkeyed: V,
}

impl<V: TrackValue> SeqTrack<V> {
    /// Whether any lane is keyed.
    pub fn is_keyed(&self) -> bool {
        self.slots.iter().any(|t| !t.keys.is_empty())
    }

    /// Sample at `clock`: interp 0 steps, else lerps; past the last key the last holds.
    pub fn sample(&self, clock: SeqClock) -> V {
        if !self.is_keyed() {
            return self.unkeyed;
        }
        let (lane, ms) = match self.global_period_ms {
            Some(period) => (
                self.slots.first(),
                if period == 0 {
                    0.0
                } else {
                    clock.scene_ms.rem_euclid(f64::from(period)) as f32
                },
            ),
            None => (
                self.slots.get(clock.slot).or(self.slots.first()),
                clock.seq_ms,
            ),
        };
        lane.filter(|t| !t.keys.is_empty())
            .map_or(self.default, |t| t.sampled_ms(ms))
    }
}

impl SeqTrack<f32> {
    /// The largest value any slot can reach, the default included where a slot is unkeyed.
    pub fn peak(&self) -> f32 {
        if !self.is_keyed() {
            return self.unkeyed;
        }
        let keyed = self.slots.iter().filter(|t| !t.keys.is_empty());
        let peak = keyed.clone().fold(f32::MIN, |m, t| m.max(t.peak()));
        if keyed.count() < self.slots.len() {
            peak.max(self.default)
        } else {
            peak
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_track_samples_linear_and_clamps() {
        let t = ValueTrack {
            keys: vec![(0, 0.0), (33, 0.0), (67, 100.0), (100, 200.0), (133, 0.0)],
            interp: 1,
        };
        assert_eq!(t.sample_ms(-5.0), 0.0); // clamp before first key
        assert_eq!(t.sample_ms(0.0), 0.0);
        assert!((t.sample_ms(50.0) - 50.0).abs() < 1.0); // rising edge, linear
        assert_eq!(t.sample_ms(100.0), 200.0); // the burst peak
        assert!((t.sample_ms(116.5) - 100.0).abs() < 1.0); // falling edge
        assert_eq!(t.sample_ms(500.0), 0.0); // clamp past last key
        assert_eq!(t.peak(), 200.0);
        assert_eq!(t.first(), 0.0);
        let c = ValueTrack {
            keys: vec![(0, 7.5)],
            interp: 0,
        };
        assert_eq!(c.sample_ms(0.0), 7.5);
        assert_eq!(c.sample_ms(9999.0), 7.5);
        assert_eq!(ValueTrack::<f32>::default().sample_ms(10.0), 0.0);
    }

    #[test]
    fn step_sampling_holds_nearest_previous() {
        let t = ValueTrack {
            keys: vec![(0, 0.0), (67, 30.0)],
            interp: 0,
        };
        assert_eq!(t.step_ms(0.0), 0.0);
        assert_eq!(
            t.step_ms(66.0),
            0.0,
            "step, not lerp — silent until the key"
        );
        assert_eq!(t.step_ms(67.0), 30.0);
        assert_eq!(t.step_ms(1500.0), 30.0, "held past the last key");
        assert_eq!(ValueTrack::<f32>::default().step_ms(10.0), 0.0);
    }

    #[test]
    fn sampled_ms_dispatches_on_the_interp_word() {
        let step = ValueTrack {
            keys: vec![(0, 0.0), (67, 30.0)],
            interp: 0,
        };
        assert_eq!(step.sampled_ms(66.0), 0.0, "interp 0: silent until the key");
        assert_eq!(step.sampled_ms(67.0), 30.0);
        let ramp = ValueTrack {
            keys: vec![(100, 10.0), (200, 110.0), (300, 0.0)],
            interp: 1,
        };
        assert!(
            (ramp.sampled_ms(150.0) - 60.0).abs() < 1e-4,
            "mid-ramp lerp"
        );
        assert_eq!(ramp.sampled_ms(200.0), 110.0);
        assert_eq!(ramp.sampled_ms(999.0), 0.0, "held past the last key");
        assert!(
            (ramp.sampled_ms(0.0) - (-90.0)).abs() < 1e-4,
            "below the first key: backward extrapolation along the first segment, not a clamp"
        );
        let single = ValueTrack {
            keys: vec![(0, 40.0)],
            interp: 1,
        };
        assert_eq!(single.sampled_ms(500.0), 40.0, "single key holds");
    }

    #[test]
    fn vec3_track_lerps_per_channel() {
        let t = ValueTrack {
            keys: vec![(0, [1.0, 0.0, 0.0]), (100, [0.0, 1.0, 0.5])],
            interp: 1,
        };
        assert_eq!(t.sample_ms(0.0), [1.0, 0.0, 0.0]);
        let mid = t.sample_ms(50.0);
        assert!((mid[0] - 0.5).abs() < 1e-6);
        assert!((mid[1] - 0.5).abs() < 1e-6);
        assert!((mid[2] - 0.25).abs() < 1e-6);
        assert_eq!(t.sample_ms(500.0), [0.0, 1.0, 0.5]);
    }
}
