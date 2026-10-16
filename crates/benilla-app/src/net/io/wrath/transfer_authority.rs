use super::*;
use benilla_protocol::wrath_transfer::{Packet, Token};

impl MovementAuthorityHandle {
    /// Called by the reader before publishing the transfer. Serialize against socket writes.
    pub(super) fn observe_transfer(&self, packet: &Packet) -> Result<Option<Token>> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch || state.self_guid != self.self_guid {
            bail!("stale Wrath transfer reader");
        }
        match packet {
            Packet::Pending { transport, .. } => {
                state.transfer_transport = transport.is_some();
                state.transfer_ack = None;
            }
            Packet::NewWorld { .. } => {
                // Transport is deliberately sticky until Pending/Aborted: a direct NEW_WORLD
                // after a boat transfer cannot establish a reviewed on-foot pose.
                state.transfer_sequence = state
                    .transfer_sequence
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("Wrath transfer sequence exhausted"))?;
                state.transfer_ack = Some(Token {
                    epoch: self.epoch,
                    sequence: state.transfer_sequence,
                });
            }
            Packet::Aborted { .. } => {
                // An abort is not permission to resume from a cached pre-transfer pose.
                state.transfer_ack = None;
                state.transfer_transport = false;
            }
        }
        state.self_created = false;
        state.self_control = false;
        state.control_decided = false;
        state.ground_pose = None;
        state.taxi_spline = None;
        state.pending_orders.clear();
        state.last_order_counter = None;
        state.rooted = false;
        state.transfer_waiting = true;
        if let Packet::NewWorld {
            position,
            orientation,
            ..
        } = packet
        {
            if !position
                .iter()
                .chain(std::iter::once(orientation))
                .all(|v| v.is_finite())
            {
                state.transfer_ack = None;
                bail!("nonfinite Wrath NEW_WORLD pose");
            }
        }
        Ok(state.transfer_ack)
    }

    /// Admission occurs after revocation and before any App-visible event.
    pub(super) fn publish_transfer(
        &self,
        packet: Packet,
        publish: impl FnOnce(SessionEvent) -> Result<()>,
    ) -> Result<()> {
        let token = self.observe_transfer(&packet)?;
        publish(SessionEvent::WrathTransfer { packet, token })
    }

    pub(super) fn send_worldport_ack(
        &self,
        token: Token,
        send: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.epoch != self.epoch
            || state.self_guid != self.self_guid
            || token.epoch != self.epoch
            || state.transfer_ack != Some(token)
        {
            return Ok(());
        }
        // Consume even if the socket fails; that session will be retired. Never grant control.
        state.transfer_ack = None;
        send()?;
        state.transfer_waiting = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn port() -> Packet {
        Packet::NewWorld {
            map_id: 1,
            position: [0.0; 3],
            orientation: 0.0,
        }
    }
    #[test]
    fn nonfinite_destinations_revoke_before_admission_without_app_emission() {
        for component in 0..4 {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let shared = Arc::new(MovementAuthority::default());
                let authority = shared.install(4, 7);
                self_create(&authority);
                let old = authority.observe_transfer(&port()).unwrap().unwrap();
                let mut pose = [0.0; 4];
                pose[component] = value;
                let packet = Packet::NewWorld {
                    map_id: 1,
                    position: [pose[0], pose[1], pose[2]],
                    orientation: pose[3],
                };
                let mut emitted = false;
                assert!(authority
                    .publish_transfer(packet, |_| {
                        emitted = true;
                        Ok(())
                    })
                    .is_err());
                assert!(!emitted);
                authority
                    .send_worldport_ack(old, || panic!("retired token"))
                    .unwrap();
                self_create(&authority);
                assert!(authority.with_movement_send(|| ()).is_none());
                let state = shared.state.lock().unwrap();
                assert!(state.transfer_waiting);
                assert!(state.ground_pose.is_none());
                assert!(state.transfer_ack.is_none());
            }
        }
    }

    #[test]
    fn failed_ack_consumes_token_but_keeps_barrier_closed() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(4, 7);
        let token = authority.observe_transfer(&port()).unwrap().unwrap();
        assert!(authority
            .send_worldport_ack(token, || Err(anyhow!("socket failed")))
            .is_err());
        self_create(&authority);
        assert!(authority.with_movement_send(|| ()).is_none());
        authority
            .send_worldport_ack(token, || panic!("retry sent twice"))
            .unwrap();
        let state = shared.state.lock().unwrap();
        assert!(state.transfer_waiting);
        assert!(!state.self_created);
    }

    fn self_create(authority: &MovementAuthorityHandle) {
        authority
            .observe_self_create(
                7,
                &MovementPosition::Stationary(benilla_protocol::wrath_gameplay::Position {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    orientation: 0.0,
                }),
            )
            .unwrap();
    }

    #[test]
    fn ack_does_not_restore_authority_but_fresh_self_create_does() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(4, 7);
        self_create(&authority);
        assert!(authority.with_movement_send(|| ()).is_some());
        let token = authority.observe_transfer(&port()).unwrap().unwrap();
        self_create(&authority); // pre-ACK creates cannot pay the barrier
        assert!(authority.with_movement_send(|| ()).is_none());
        authority.send_worldport_ack(token, || Ok(())).unwrap();
        assert!(authority.with_movement_send(|| ()).is_none());
        self_create(&authority);
        assert!(authority.with_movement_send(|| ()).is_some());
    }

    #[test]
    fn transport_create_and_ack_never_grant_on_foot_authority() {
        let shared = Arc::new(MovementAuthority::default());
        let authority = shared.install(4, 7);
        self_create(&authority);
        authority
            .observe_transfer(&Packet::Pending {
                map_id: 1,
                transport: Some(benilla_protocol::wrath_transfer::Transport {
                    entry: 99,
                    source_map: 0,
                }),
            })
            .unwrap();
        let token = authority.observe_transfer(&port()).unwrap().unwrap();
        authority.send_worldport_ack(token, || Ok(())).unwrap();
        self_create(&authority);
        assert!(authority.with_movement_send(|| ()).is_none());
        assert!(shared.state.lock().unwrap().ground_pose.is_none());
    }

    #[test]
    fn double_new_world_rejects_delayed_ack_and_consumes_current_once() {
        let shared = Arc::new(MovementAuthority {
            state: Mutex::new(Default::default()),
        });
        let authority = shared.install(4, 7);
        let old = authority.observe_transfer(&port()).unwrap().unwrap();
        let current = authority.observe_transfer(&port()).unwrap().unwrap();
        assert!(current.sequence > old.sequence);
        let mut sends = 0;
        authority
            .send_worldport_ack(old, || {
                sends += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(sends, 0);
        authority
            .send_worldport_ack(current, || {
                sends += 1;
                Ok(())
            })
            .unwrap();
        authority
            .send_worldport_ack(current, || {
                sends += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(sends, 1);
        assert!(authority.with_movement_send(|| ()).is_none());
        assert!(shared.state.lock().unwrap().ground_pose.is_none());
    }
    #[test]
    fn epoch_and_pending_retire_old_capabilities() {
        let shared = Arc::new(MovementAuthority {
            state: Mutex::new(Default::default()),
        });
        let old = shared.install(4, 7);
        let token = old.observe_transfer(&port()).unwrap().unwrap();
        let new = shared.install(5, 7);
        let current = new.observe_transfer(&port()).unwrap().unwrap();
        old.send_worldport_ack(token, || panic!("stale handle"))
            .unwrap();
        new.send_worldport_ack(token, || panic!("stale token"))
            .unwrap();
        new.observe_transfer(&Packet::Pending {
            map_id: 2,
            transport: None,
        })
        .unwrap();
        new.send_worldport_ack(current, || panic!("retired token"))
            .unwrap();
    }
}
