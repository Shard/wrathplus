use super::*;

impl MovementAuthorityHandle {
    // Native 006b1930 only sets the pending flag. Server root/unroot owns freezing;
    // neither request nor cancel may revoke or recreate ground/taxi authority.
    pub(super) fn send_logout(&self, send: impl FnOnce() -> Result<()>) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid || state.logout_pending {
            return Ok(());
        }
        state.logout_pending = true;
        send()
    }

    pub(super) fn send_logout_cancel(&self, send: impl FnOnce() -> Result<()>) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid {
            return Ok(());
        }
        // Native 006b18c0 clears immediately without LOGOUT_CANCEL; 006b0900
        // emits that event only if pending remains set when the untagged ACK arrives.
        state.logout_pending = false;
        send()
    }

    pub(super) fn observe_taxi_spline(&self, event: &SessionEvent) {
        let SessionEvent::MonsterMove {
            guid,
            spline_id,
            flying,
            stop,
            cyclic,
            transport,
            ..
        } = event
        else {
            return;
        };
        if *guid != self.self_guid {
            return;
        }
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid {
            return;
        }
        // Preserve the consumed marker for a duplicate delivery of the same server spline.
        if !state.transfer_waiting
            && !state.transfer_transport
            && *flying
            && !stop
            && !cyclic
            && transport.is_none()
        {
            if state.taxi_spline.map(|(id, _)| id) != Some(*spline_id) {
                state.taxi_spline = Some((*spline_id, false));
            }
        } else {
            state.taxi_spline = None;
        }
    }

    pub(super) fn send_taxi_done(
        &self,
        flags: u32,
        pos: [f32; 3],
        orientation: f32,
        spline_id: u32,
        timestamp: u32,
        send: impl FnOnce(u32, &[u8]) -> Result<()>,
    ) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch
            || state.self_guid != self.self_guid
            || !state.self_created
            || flags != 0
            || state.taxi_spline != Some((spline_id, false))
        {
            return Ok(());
        }
        let body = wrathplus_protocol::wrath_taxi_ack::encode(
            self.self_guid,
            pos,
            orientation,
            timestamp,
            spline_id,
        )?;
        // This permission is independent of on-foot/control grants and is consumed even if the
        // socket fails. The writer's existing failure path retires that session.
        state.taxi_spline = Some((spline_id, true));
        send(
            wrathplus_protocol::wrath_taxi_ack::CMSG_MOVE_SPLINE_DONE,
            &body,
        )
    }
}

#[cfg(test)]
#[path = "taxi_authority_tests.rs"]
mod tests;
