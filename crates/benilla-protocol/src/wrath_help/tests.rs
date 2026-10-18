use super::*;

fn w(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn f(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn fixtures() -> Vec<(u16, Vec<u8>, SessionEvent)> {
    let c = |packet| SessionEvent::WrathHelp { packet };
    vec![
        (0x206, w(2), SessionEvent::GmTicketCreated { response: 2 }),
        (0x208, w(5), SessionEvent::GmTicketUpdated { response: 5 }),
        (0x218, w(9), SessionEvent::GmTicketDeleted { response: 9 }),
        (
            0x21b,
            w(1),
            SessionEvent::GmTicketSystemStatus { status: 1 },
        ),
        (0x212, w(0x0a), SessionEvent::GmTicket { ticket: None }),
        (
            0x212,
            [
                w(6),
                w(77),
                b"stuck\0".to_vec(),
                vec![1],
                f(0.5),
                f(2.0),
                f(0.01),
                vec![2, 1],
            ]
            .concat(),
            SessionEvent::GmTicket {
                ticket: Some(Box::new(GmTicket {
                    text: "stuck".into(),
                    category: 0,
                    ticket_age: 0.5,
                    oldest_ticket_age: 2.0,
                    update_time: 0.01,
                    assigned_to_gm: 2,
                    opened_by_gm: 1,
                })),
            },
        ),
        (
            0x4ef,
            [
                w(1),
                w(77),
                b"stuck\0".to_vec(),
                b"try again\0\0\0\0".to_vec(),
            ]
            .concat(),
            c(Packet::GmResponse {
                response_id: 1,
                ticket_id: 77,
                ticket_text: "stuck".into(),
                response: [
                    "try again".into(),
                    String::new(),
                    String::new(),
                    String::new(),
                ],
            }),
        ),
        (
            0x4f1,
            vec![1],
            c(Packet::GmResponseStatus { show_survey: 1 }),
        ),
        (0x3c8, vec![0], c(Packet::ComplainResult { raw: 0 })),
    ]
}

#[test]
fn every_fixture_decodes_and_rejects_bad_extent() {
    let fixtures = fixtures();
    let covered: std::collections::BTreeSet<_> = fixtures.iter().map(|f| f.0).collect();
    assert_eq!(covered, OPCODES.into_iter().collect());
    let mut adapter = crate::wrath_events::WrathEventAdapter::new();
    for (opcode, body, expected) in fixtures {
        assert_eq!(
            format!("{:?}", decode(opcode, &body).unwrap()),
            format!("{expected:?}"),
            "{opcode:#x}"
        );
        for end in 0..body.len() {
            assert!(
                decode(opcode, &body[..end]).is_err(),
                "{opcode:#x} truncated at {end}"
            );
        }
        assert!(
            decode(opcode, &[body.as_slice(), &[0]].concat()).is_err(),
            "{opcode:#x} trailing"
        );
        match adapter.dispatch(opcode, &body, 0).unwrap() {
            crate::wrath_events::WrathDispatch::Handled { session_events, .. } => {
                assert_eq!(
                    format!("{session_events:?}"),
                    format!("{:?}", vec![expected])
                )
            }
            other => panic!("{opcode:#x}: {other:?}"),
        }
    }
    assert!(
        decode(0x212, &w(3)).is_err(),
        "only the default and has-text statuses"
    );
}

#[test]
fn commands_encode_ac_read_order() {
    let create = [
        w(0),
        f(1.0),
        f(2.0),
        f(3.0),
        b"help\0".to_vec(),
        w(1),
        vec![0],
        w(0),
        w(0),
    ]
    .concat();
    let survey = [
        w(9),
        w(30),
        vec![4],
        b"ok\0".to_vec(),
        w(0),
        b"thanks\0".to_vec(),
    ]
    .concat();
    for (command, opcode, body) in [
        (
            Command::Create {
                map: 0,
                pos: [1.0, 2.0, 3.0],
                text: "help".into(),
                need_response: true,
                need_more_help: false,
            },
            0x205,
            create,
        ),
        (
            Command::UpdateText {
                text: "more".into(),
            },
            0x207,
            b"more\0".to_vec(),
        ),
        (Command::GetTicket, 0x211, vec![]),
        (Command::DeleteTicket, 0x217, vec![]),
        (Command::SystemStatus, 0x21a, vec![]),
        (
            Command::SurveySubmit {
                survey: 9,
                answers: vec![SurveyAnswer {
                    question: 30,
                    rank: 4,
                    comment: "ok".into(),
                }],
                comment: "thanks".into(),
            },
            0x32a,
            survey,
        ),
        (Command::ResolveResponse, 0x4f0, vec![]),
        (
            Command::ComplainMail {
                spammer: 5,
                mail_id: 12,
            },
            0x3c7,
            [vec![0], 5u64.to_le_bytes().to_vec(), w(0), w(12), w(0)].concat(),
        ),
        (
            Command::ComplainChat {
                spammer: 5,
                language: 7,
                message_type: 1,
                channel_id: 0,
                seconds_since: 30,
                description: "spam".into(),
            },
            0x3c7,
            [
                vec![1],
                5u64.to_le_bytes().to_vec(),
                w(7),
                w(1),
                w(0),
                w(30),
                b"spam\0".to_vec(),
            ]
            .concat(),
        ),
        (
            Command::Bug {
                suggestion: true,
                content: "abc".into(),
                kind: "UI".into(),
            },
            0x1ca,
            [w(1), w(3), b"abc\0".to_vec(), w(2), b"UI\0".to_vec()].concat(),
        ),
    ] {
        assert_eq!(encode(&command).unwrap(), (opcode, body), "{command:?}");
    }
    // Ten answers leave no terminator; AC reads exactly ten.
    let ten = Command::SurveySubmit {
        survey: 1,
        answers: (1..=10)
            .map(|q| SurveyAnswer {
                question: q,
                rank: 0,
                comment: String::new(),
            })
            .collect(),
        comment: String::new(),
    };
    assert_eq!(encode(&ten).unwrap().1.len(), 4 + 10 * 6 + 1);
    for bad in [
        Command::UpdateText {
            text: "a\0b".into(),
        },
        Command::SurveySubmit {
            survey: 1,
            answers: vec![SurveyAnswer {
                question: 0,
                rank: 0,
                comment: String::new(),
            }],
            comment: String::new(),
        },
    ] {
        assert!(encode(&bad).is_err(), "{bad:?}");
    }
}
