use super::*;
use benilla_protocol::wrath_guild_bank::{Packet, Permissions, TabPermission};
use benilla_protocol::wrath_guild_bank_query::Query;

#[test]
fn native_queries_have_exact_bodies_and_no_mutation_route() {
    for (query, opcode, body) in [
        (Query::Permissions, 0x03fd, vec![]),
        (Query::RemainingMoney, 0x03fe, vec![]),
        (
            Query::Tab {
                banker: 0x0102030405060708,
                tab: 5,
                full_update: 255,
            },
            0x03e7,
            vec![8, 7, 6, 5, 4, 3, 2, 1, 5, 255],
        ),
    ] {
        let command = ClientCommand::GuildBankQuery(query);
        assert_eq!(route(&command), WrathRoute::NativeSession);
        assert_eq!(session_packet(command), (opcode, body));
    }
}

fn permissions_body(count: u8) -> Vec<u8> {
    let mut body = vec![2, 0, 0, 0, 0, 0, 4, 128, 255, 255, 255, 255, count];
    for row in 0u32..6 {
        body.extend((0x80000000 | row).to_le_bytes());
        body.extend((u32::MAX - row).to_le_bytes());
    }
    body
}

#[test]
fn adapter_preserves_fixed_rows_raw_words_and_signed_remaining() {
    let mut adapter = WrathEventAdapter::default();
    for count in [0, 1, 6, 255] {
        let WrathDispatch::Handled {
            session_events: events,
            ..
        } = adapter
            .dispatch(0x03fd, &permissions_body(count), 0)
            .unwrap()
        else {
            panic!()
        };
        let [SessionEvent::WrathGuildBank { packet }] = events.as_slice() else {
            panic!()
        };
        assert_eq!(
            packet,
            &Packet::Permissions(Permissions {
                rank: 2,
                flags: 0x80040000,
                money_limit: u32::MAX,
                purchased_tabs: count,
                tabs: std::array::from_fn(|row| TabPermission {
                    rights: 0x80000000 | row as u32,
                    remaining_items: u32::MAX - row as u32,
                }),
            })
        );
    }
    for remaining in [0i32, -1, i32::MIN, i32::MAX] {
        let WrathDispatch::Handled {
            session_events: events,
            ..
        } = adapter
            .dispatch(0x03fe, &remaining.to_le_bytes(), 0)
            .unwrap()
        else {
            panic!()
        };
        let [SessionEvent::WrathGuildBank { packet }] = events.as_slice() else {
            panic!()
        };
        assert_eq!(packet, &Packet::RemainingMoney { remaining });
    }
}

#[test]
fn malformed_guild_bank_replies_never_emit_events() {
    let mut adapter = WrathEventAdapter::default();
    for (opcode, body) in [(0x03fd, permissions_body(255)), (0x03fe, vec![255; 4])] {
        for end in 0..body.len() {
            assert!(
                matches!(adapter.dispatch(opcode, &body[..end], 0).unwrap(), WrathDispatch::Unhandled { opcode: actual } if actual == opcode)
            );
        }
        let mut tail = body;
        tail.push(0);
        assert!(
            matches!(adapter.dispatch(opcode, &tail, 0).unwrap(), WrathDispatch::Unhandled { opcode: actual } if actual == opcode)
        );
    }
}
