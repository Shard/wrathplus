//! The death-knight rune bar's clock: each rune's cooldown start on the `GetTime` clock, as the
//! client keeps it for `GetRuneCooldown` (`0x613020`), timed from the retained rune packets
//! ([`NativeRunes`]) and refilled locally at `1 / PLAYER_RUNE_REGEN[type]` seconds.
//!
//! A packet rebases every rune: `passed` of 255 is ready, otherwise the cooldown began
//! `passed / 255` of a duration ago. Between packets a rune comes up when its duration has run.
//! `RUNE_TYPE_UPDATE(slot)` fires on a type change, `RUNE_POWER_UPDATE(slot, usable)` when a rune
//! goes down or comes up (`RuneFrame.lua:131-142`).
//!
//! Unresolved, labelled: a regen-rate change mid-cooldown keeps the start and takes the new
//! duration; the reference's own refill tick and its event sites are untraced.

use bevy::prelude::*;

use wrathplus_protocol::types::Field;
use wrathplus_ui::script::{RuneView, ScriptValue, UiScript, WrathRunes};

use crate::app_core::schedule::UnitFeed;
use crate::app_core::session::ObjectStore;
use crate::app_core::session::SelfPlayer;
use crate::net::wrath_runes::{NativeRunes, RUNES};

pub struct UiRunesPlugin;

impl Plugin for UiRunesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, feed_runes.in_set(UnitFeed));
    }
}

/// `1 / regen`, 0 for a rate at or below zero (`0x6130b6` tests `<= 0`).
fn duration(regen: f32) -> f32 {
    if regen > 0.0 {
        1.0 / regen
    } else {
        0.0
    }
}

/// The rune's start after a packet: ready at 255 or with no duration, else `passed / 255` run.
fn rebased_start(passed: u8, duration: f32, now: f64) -> Option<f64> {
    (passed < u8::MAX && duration > 0.0)
        .then(|| now - f64::from(passed) / 255.0 * f64::from(duration))
}

/// Advance the snapshot to `now`: a rune whose duration has run comes up.
fn refill(runes: &mut [RuneView; RUNES], now: f64) {
    for r in runes {
        if r.start.is_some_and(|s| s + f64::from(r.duration) <= now) {
            r.start = None;
        }
    }
}

#[derive(Default)]
struct Memory {
    runes: Option<[RuneView; RUNES]>,
}

fn feed_runes(
    script: Option<NonSendMut<UiScript>>,
    native: Res<NativeRunes>,
    me: Query<&ObjectStore, With<SelfPlayer>>,
    clock: Res<crate::app_core::ui_flags::UiClock>,
    mut memory: Local<crate::app_core::vm_memo::VmMemo<Memory>>,
) {
    let Some(mut script) = script else { return };
    let (memory, vm_reset) = memory.get_reset(&script);
    let now = clock.ui_now;
    let regen = |rune_type: u8| {
        me.iter()
            .next()
            .and_then(|s| s.0.get_float(Field::PlayerRuneRegen1, u16::from(rune_type)))
            .unwrap_or(0.0)
    };
    let last = memory.runes;
    let mut fresh = match native.0 {
        None => None,
        Some(wire) if native.is_changed() || vm_reset || last.is_none() => {
            Some(std::array::from_fn(|i| {
                let d = duration(regen(wire[i].rune_type));
                RuneView {
                    rune_type: wire[i].rune_type,
                    start: rebased_start(wire[i].passed, d, now),
                    duration: d,
                }
            }))
        }
        Some(_) => last.map(|mut runes| {
            for r in &mut runes {
                r.duration = duration(regen(r.rune_type));
            }
            runes
        }),
    };
    if let Some(runes) = fresh.as_mut() {
        refill(runes, now);
    }
    if fresh == last && !vm_reset {
        return;
    }
    memory.runes = fresh;
    script.set_wrath_runes(WrathRunes(fresh));
    let (Some(last), Some(fresh)) = (last, fresh) else {
        return;
    };
    for (i, (was, now)) in last.iter().zip(&fresh).enumerate() {
        let slot = ScriptValue::Int(i as i64 + 1);
        if was.rune_type != now.rune_type {
            script.fire_event("RUNE_TYPE_UPDATE", vec![slot.clone()]);
        }
        if was.start.is_none() != now.start.is_none() {
            script.fire_event(
                "RUNE_POWER_UPDATE",
                vec![slot, ScriptValue::Bool(now.start.is_none())],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rebased_rune_refills_after_its_duration() {
        assert_eq!(duration(0.1), 10.0);
        assert_eq!(duration(0.0), 0.0);
        assert_eq!(rebased_start(255, 10.0, 100.0), None);
        assert_eq!(rebased_start(0, 0.0, 100.0), None, "no rate is no cooldown");
        let start = rebased_start(51, 10.0, 100.0).unwrap();
        assert!((start - 98.0).abs() < 1e-9, "a fifth run: began 2 s ago");
        let mut runes = [RuneView {
            rune_type: 0,
            start: Some(start),
            duration: 10.0,
        }; RUNES];
        refill(&mut runes, 107.9);
        assert!(runes[0].start.is_some());
        refill(&mut runes, 108.0);
        assert!(runes.iter().all(|r| r.start.is_none()));
    }

    /// The feed end to end: a resync rebases, a spend takes runes down with `RUNE_POWER_UPDATE`
    /// false, the clock brings them back with true, and a convert fires `RUNE_TYPE_UPDATE`.
    #[test]
    fn the_feed_times_runes_and_fires_the_rune_events() {
        use crate::net::wrath_runes::Rune;
        let mut app = App::new();
        let script = UiScript::new().unwrap();
        script
            .run(
                r#"seen = {} local f = CreateFrame("Frame")
                f:RegisterEvent("RUNE_POWER_UPDATE") f:RegisterEvent("RUNE_TYPE_UPDATE")
                f:SetScript("OnEvent", function(_, e, a, b) seen[#seen+1] = e .. ":" .. tostring(a) .. ":" .. tostring(b) end)"#,
            )
            .unwrap();
        app.insert_non_send_resource(script)
            .init_resource::<NativeRunes>()
            .init_resource::<crate::app_core::ui_flags::UiClock>()
            .add_systems(Update, feed_runes);
        let regen: Vec<(Field, u16, u32)> = (0..4)
            .map(|t| (Field::PlayerRuneRegen1, t, 0.1f32.to_bits()))
            .collect();
        app.world_mut().spawn((
            SelfPlayer,
            ObjectStore(wrathplus_protocol::ObjectFields::from_field_slots(&regen)),
        ));
        let step = |app: &mut App, now: f64, runes: Option<[Rune; RUNES]>| {
            app.world_mut()
                .resource_mut::<crate::app_core::ui_flags::UiClock>()
                .ui_now = now;
            if let Some(r) = runes {
                app.world_mut().resource_mut::<NativeRunes>().0 = Some(r);
            }
            app.update();
            let s = app.world().non_send_resource::<UiScript>();
            let seen = s
                .eval::<String>("local out = table.concat(seen, ' ') seen = {} return out")
                .unwrap();
            let cd = s
                .eval::<(f64, f64, bool)>("return GetRuneCooldown(1)")
                .unwrap();
            (seen, cd)
        };
        let ready = [Rune {
            rune_type: 0,
            passed: 255,
        }; RUNES];
        let (seen, cd) = step(&mut app, 100.0, Some(ready));
        assert_eq!((seen.as_str(), cd), ("", (0.0, 10.0, true)));
        let mut spent = ready;
        spent[0].passed = 0;
        let (seen, cd) = step(&mut app, 101.0, Some(spent));
        assert_eq!(seen, "RUNE_POWER_UPDATE:1:false");
        assert_eq!(cd, (101.0, 10.0, false));
        let (seen, _) = step(&mut app, 110.5, None);
        assert_eq!(seen, "");
        let (seen, cd) = step(&mut app, 111.0, None);
        assert_eq!(seen, "RUNE_POWER_UPDATE:1:true");
        assert!(cd.2);
        let mut death = ready;
        death[4].rune_type = 3;
        let (seen, _) = step(&mut app, 112.0, Some(death));
        assert_eq!(seen, "RUNE_TYPE_UPDATE:5:nil");
        let s = app.world().non_send_resource::<UiScript>();
        assert_eq!(s.eval::<i64>("return GetRuneType(5)").unwrap(), 4);
    }
}
