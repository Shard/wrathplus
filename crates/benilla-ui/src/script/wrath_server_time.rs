//! Build-12340 server clock for Lua readers: the `WORLD_STATE_UI_TIMER_UPDATE` reply's offset.
//! Evidence: one headless Ghidra round on the pinned Wow.exe (`/tmp/azb-app-mig/ghidra/
//! ui-timer{,2,3}.log`).
//!
//! - The reply handler stores `offset = server_time - time()` (`0x5486e0`, `DAT_00be8f44`); the
//!   world reset `0x548570` zeroes it. `server_now` (`0x548700`) is `time() + offset`.
//! - Its readers are the Wintergrasp wait (`0x5435a0`: world state `0x1102` minus now, clamped at
//!   0), `GetWorldPVPQueueStatus`'s confirm countdown (`0x54c8a0`) and the world-state timer text
//!   (`0x576e50`, `%02d:%02d` / `%d:%02d:%02d`), which Native UI builds on [`server_now`].
//! - The App owns the query cadence (`0x5488f0`: one `CMSG 0x4F6` every 20 s, the first at once
//!   after the reset) and pushes each offset here. Deviation: before the first reply the offset
//!   is unknown (`None`), where the reference reads 0 (local time).
use super::{Model, UiScript};

impl UiScript {
    /// The server clock offset in seconds (`server - time()`), `None` until the first reply.
    pub fn set_wrath_server_time_offset(&mut self, offset: Option<i32>) {
        self.model_mut().wrath_server_time_offset = offset;
    }

    /// The server's Unix time now (`0x548700`), `None` while the offset is unknown.
    pub fn wrath_server_now(&self) -> Option<i64> {
        server_now(&self.model_ref())
    }
}

/// `0x548700`: local `time()` plus the stored offset; `None` while the offset is unknown.
pub(crate) fn server_now(model: &Model) -> Option<i64> {
    let offset = model.wrath_server_time_offset?;
    let local = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    Some(local + i64::from(offset))
}

#[cfg(test)]
mod tests {
    use crate::script::UiScript;

    #[test]
    fn server_now_is_local_time_plus_the_pushed_offset() {
        let mut s = UiScript::new().unwrap();
        assert_eq!(s.wrath_server_now(), None);
        let local = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        s.set_wrath_server_time_offset(Some(-3600));
        let now = s.wrath_server_now().unwrap();
        assert!((now - (local - 3600)).abs() <= 1, "{now} vs {local}");
        s.set_wrath_server_time_offset(None);
        assert_eq!(s.wrath_server_now(), None);
    }
}
