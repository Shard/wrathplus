use super::packet::{packet_aura_changes, PacketAuraChange};
use super::*;

#[cfg(test)]
mod native_death_tests {
    use super::*;
    use benilla_protocol::messages::{Field, FieldLayout, ObjectType};

    #[test]
    fn death_watch_resolves_native_health_and_preserves_alive_to_dead_gate() {
        let layout = FieldLayout::Wrath12340;
        for kind in [ObjectType::Unit, ObjectType::Player] {
            let span = layout.resolve(Field::UnitHealth).unwrap();
            let (field, offset) = layout.field_at(kind, span.index).unwrap();
            let mut edge = FieldChanged {
                entity: Entity::PLACEHOLDER,
                guid: 42,
                kind,
                layout,
                field,
                offset,
                old: 1,
                new: 0,
            };
            assert!(is_death_edge(&edge));
            for (old, new) in [(0, 0), (0, 1), (2, 1)] {
                edge.old = old;
                edge.new = new;
                assert!(!is_death_edge(&edge));
            }
            edge.old = 1;
            edge.new = 0;
            edge.kind = ObjectType::Corpse;
            assert!(!is_death_edge(&edge));
            edge.kind = kind;
            let span = layout.resolve(Field::UnitMaxHealth).unwrap();
            (edge.field, edge.offset) = layout.field_at(kind, span.index).unwrap();
            assert!(!is_death_edge(&edge));
        }
    }
}

#[cfg(test)]
mod packet_aura_tests {
    use super::*;
    use benilla_protocol::wrath_auras::NativeAura;

    fn aura(slot: u8, spell_id: u32, stacks: u8) -> NativeAura {
        NativeAura {
            slot,
            spell_id,
            stacks,
            flags: 0x10,
            level: 80,
            caster_guid: None,
            max_duration_ms: Some(1000),
            remaining_ms: Some(900),
        }
    }

    #[test]
    fn native_slots_add_replace_remove_and_stack_without_vanilla_slot_limits() {
        use PacketAuraChange::*;
        let a = aura(200, 42, 1);
        assert_eq!(
            packet_aura_changes(&[], std::slice::from_ref(&a)),
            vec![Added(42, true)]
        );
        let mut stacked = a.clone();
        stacked.stacks = 3;
        assert_eq!(
            packet_aura_changes(std::slice::from_ref(&a), &[stacked.clone()]),
            vec![Stacked(42, true, 3)]
        );
        assert!(packet_aura_changes(&[stacked], std::slice::from_ref(&a)).is_empty());
        let mut replacement = aura(200, 99, 4);
        replacement.flags = 0x80;
        assert_eq!(
            packet_aura_changes(std::slice::from_ref(&a), &[replacement]),
            vec![Added(99, false), Removed(42)]
        );
        assert_eq!(packet_aura_changes(&[a], &[]), vec![Removed(42)]);
        assert!(packet_aura_changes(&[], &[]).is_empty());
    }

    #[test]
    fn refreshes_and_metadata_do_not_repeat_lines_and_polarity_comes_from_flags() {
        use PacketAuraChange::*;
        let a = aura(0, 42, 1);
        let mut refresh = a.clone();
        refresh.remaining_ms = Some(1000);
        refresh.caster_guid = Some(77);
        refresh.flags = 0x80;
        refresh.level = 79;
        assert!(packet_aura_changes(&[a], &[refresh.clone()]).is_empty());
        refresh.stacks = 2;
        assert_eq!(packet_aura_changes(&[], &[refresh]), vec![Added(42, false)]);
    }
}

#[cfg(test)]
mod packet_aura_system_tests {
    use super::*;
    use crate::net::wrath_auras::{PacketAuras, StampedAura};
    use benilla_protocol::messages::{FieldLayout, ObjectFields, ObjectType};
    use benilla_protocol::wrath_auras::NativeAura;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.insert_resource(SelfGuid(Some(42)))
            .init_resource::<GroupState>()
            .init_resource::<GuidIndex>()
            .init_resource::<Reputations>()
            .init_resource::<CombatLogRanges>()
            .init_resource::<ChatLog>()
            .init_resource::<PacketAuras>()
            .add_message::<FieldChanged>()
            .add_systems(Update, aura_lines);
        let entity = app
            .world_mut()
            .spawn(ObjectStore(
                ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &[])
                    .into_created(ObjectType::Player),
            ))
            .id();
        app.world_mut()
            .resource_mut::<GuidIndex>()
            .0
            .insert(42, entity);
        (app, entity)
    }

    fn set(app: &mut App, stacks: Option<u8>) {
        let auras = stacks
            .map(|stacks| StampedAura {
                received_at: 1.0,
                aura: NativeAura {
                    slot: 200,
                    spell_id: 77,
                    flags: 0x10,
                    level: 80,
                    stacks,
                    caster_guid: None,
                    max_duration_ms: None,
                    remaining_ms: None,
                },
            })
            .into_iter()
            .collect();
        app.world_mut()
            .resource_mut::<PacketAuras>()
            .by_guid
            .insert(42, auras);
    }

    fn take(app: &mut App) -> usize {
        app.update();
        let mut log = app.world_mut().resource_mut::<ChatLog>();
        let count = log.pending_len();
        log.clear_session();
        count
    }

    #[test]
    fn a_packet_snapshot_adds_once_and_an_authoritative_empty_removes_once() {
        let (mut app, _) = app();
        set(&mut app, Some(1));
        assert_eq!(take(&mut app), 1, "first packet adds once");
        assert_eq!(take(&mut app), 0, "held snapshot is quiet");
        set(&mut app, Some(3));
        assert_eq!(take(&mut app), 1, "native stacks rise");
        set(&mut app, None);
        assert_eq!(take(&mut app), 1, "authoritative empty removes once");
        assert_eq!(take(&mut app), 0);
    }

    #[test]
    fn packet_lines_keep_spell_name_attribute_classification_and_range_gates() {
        for (name, attributes, range, expected) in [
            ("Visible", 0, 100_000.0, 1),
            ("", 0, 100_000.0, 0),
            ("Hidden", 0x80, 100_000.0, 0),
            ("Hidden", 0x100, 100_000.0, 0),
            ("Visible", 0, 0.0, 0),
        ] {
            let (mut app, _) = app();
            let mut spells = crate::ui_action::Spells::empty_for_tests();
            spells.catalog = benilla_formats::SpellCatalog::from_displays(
                [(
                    77,
                    benilla_formats::SpellDisplay {
                        name: name.into(),
                        attributes,
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
            );
            app.insert_resource(spells);
            app.world_mut().resource_mut::<CombatLogRanges>().class[UnitClass::Me as usize] = range;
            set(&mut app, Some(1));
            assert_eq!(take(&mut app), expected, "{name}/{attributes:#x}/{range}");
        }
        let (mut app, _) = app();
        app.world_mut().resource_mut::<SelfGuid>().0 = None;
        set(&mut app, Some(1));
        assert_eq!(
            take(&mut app),
            0,
            "unknown classification has no periodic line"
        );
    }

    #[test]
    fn missing_objects_non_units_and_despawn_are_quiet() {
        let (mut app, entity) = app();
        app.world_mut().entity_mut(entity).insert(ObjectStore(
            ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &[])
                .into_created(ObjectType::GameObject),
        ));
        set(&mut app, Some(1));
        assert_eq!(take(&mut app), 0, "non-unit gate");
        app.world_mut().entity_mut(entity).insert(ObjectStore(
            ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &[])
                .into_created(ObjectType::Player),
        ));
        assert_eq!(take(&mut app), 1);
        app.world_mut()
            .resource_mut::<PacketAuras>()
            .by_guid
            .clear();
        app.world_mut().despawn(entity);
        app.world_mut().resource_mut::<GuidIndex>().0.clear();
        assert_eq!(take(&mut app), 0, "despawn is not an aura-removal line");
        set(&mut app, Some(1));
        assert_eq!(take(&mut app), 0, "missing object");
    }
}
