//! The clocks Lua reads: `GetTime()`, the session seconds [`UiScript::tick`] advances, read live
//! between ticks once the host turns that on; `GetGameTime()`, the game clock the host pushes; and
//! the `debugprofilestart`/`debugprofilestop` pair, a live timer. The first two live in the
//! [`Model`], out of an addon's reach, as the reference's live in the client.
//!
//! [`UiScript::tick`]: super::UiScript::tick

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use mlua::Lua;

use super::Model;

/// `GetTime()`'s live reading between ticks: the instant the frame clock last moved, and the last
/// answer, which the next one never undercuts.
pub(crate) struct LiveClock {
    anchor: Instant,
    last: f64,
}

impl super::UiScript {
    /// The current `GetTime()` value in seconds; the app stamps absolute expiries with it, such as
    /// an aura's `expirationTime`.
    pub fn now(&self) -> f64 {
        self.model_ref().now
    }

    /// Start this VM's `GetTime()` clock at `secs`, so a rebuilt VM (a relog, a `ReloadUI`) keeps
    /// the process's clock. The reference's `GetTime` (`0x6081f0`) is the OS clock scaled by 0.001,
    /// which never restarts, and stock `Cooldown.lua` relies on that (`start > 0`). Set once, at
    /// construction; after that only [`Self::tick`] moves it.
    pub fn set_now(&mut self, secs: f64) {
        let mut model = self.model_mut();
        model.now = secs;
        if let Some(live) = &mut model.live_clock {
            *live = LiveClock {
                anchor: Instant::now(),
                last: secs,
            };
        }
    }

    /// Read `GetTime()` live: build 12340's (`0x6081f0`) reads the OS clock at every call
    /// (`0x86ae20` -> `0x86adc0`, `GetTickCount` or `QueryPerformanceCounter`, in whole
    /// milliseconds), so it moves inside a frame. Off, it answers the frame clock alone, which the
    /// host's own stamps (cooldowns, aura expiries) keep using either way.
    pub fn set_live_clock(&mut self, on: bool) {
        let mut model = self.model_mut();
        model.live_clock = on.then(|| LiveClock {
            anchor: Instant::now(),
            last: model.now,
        });
    }

    /// Push the game clock `GetGameTime()` answers, `(hour, minute)`; 0:00 until the first push.
    pub fn set_game_time(&mut self, hour: u32, minute: u32) {
        self.model_mut().game_time = (hour, minute);
    }
}

/// `GetTime()`'s clock, for a caller holding no model borrow; one that holds it reads `model.now`.
pub(crate) fn now(lua: &Lua) -> f64 {
    lua.app_data_ref::<Model>().expect("model app_data").now
}

/// The frame clock moved: a live `GetTime()` counts from here.
pub(super) fn reanchor(model: &mut Model) {
    if let Some(live) = &mut model.live_clock {
        live.anchor = Instant::now();
    }
}

/// `GetTime()`'s answer: the frame clock plus, when live, the whole milliseconds since it moved.
fn get_time(model: &mut Model) -> f64 {
    let now = model.now;
    let Some(live) = &mut model.live_clock else {
        return now;
    };
    let since = live.anchor.elapsed().as_millis() as f64 * 0.001;
    live.last = live.last.max(now + since);
    live.last
}

/// Register `GetTime`, `GetGameTime` and the profile pair.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // GetTime() (`0x6081f0`): one number, seconds. Ours is the session clock the tick advances, so
    // `GetTime()` deltas and the `elapsed` handed to OnUpdate agree, read live between ticks.
    g.set(
        "GetTime",
        lua.create_function(|lua, ()| {
            Ok(get_time(
                &mut lua.app_data_mut::<Model>().expect("model app_data"),
            ))
        })?,
    )?;
    // `debugprofilestart` (`0x817350`) latches the timer and answers nothing; `debugprofilestop`
    // (`0x817370`) answers the milliseconds since, fractional, read live at the call. Before any
    // start the latch is the timer's epoch, here this VM's install.
    let start = Rc::new(Cell::new(Instant::now()));
    let latch = start.clone();
    g.set(
        "debugprofilestart",
        lua.create_function(move |_, ()| {
            latch.set(Instant::now());
            Ok(())
        })?,
    )?;
    g.set(
        "debugprofilestop",
        lua.create_function(move |_, ()| Ok(start.get().elapsed().as_secs_f64() * 1000.0))?,
    )?;
    // GetGameTime() (`0x608230`): two numbers, the hour `[0xd37f9c]` then the minute
    // `[0xd37f98]`, the realm's game clock; minute resolution, no seconds.
    g.set(
        "GetGameTime",
        lua.create_function(|lua, ()| {
            let (hour, minute) = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .game_time;
            Ok((f64::from(hour), f64::from(minute)))
        })?,
    )?;
    Ok(())
}
