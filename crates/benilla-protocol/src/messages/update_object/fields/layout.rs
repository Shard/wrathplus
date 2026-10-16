//! Native descriptor metadata. Table indexing, not a name/hash lookup, is used for reads.
use super::ObjectType;

#[path = "layout_generated.rs"]
mod generated;
pub use generated::Field;

/// The native dword span of a semantic field or array root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpan {
    pub index: u16,
    pub width: u16,
    pub object_type: ObjectType,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldLayout {
    #[default]
    Vanilla5875,
    Wrath12340,
}

impl FieldLayout {
    #[inline]
    pub fn resolve(self, field: Field) -> Option<FieldSpan> {
        match self {
            Self::Vanilla5875 => generated::VANILLA[field as usize],
            Self::Wrath12340 => generated::WRATH[field as usize],
        }
    }

    /// The first dword index of `field` on this layout, for fixtures and watch constants that
    /// still key on an index. A field the layout lacks is a compile-time error in a `const`.
    pub const fn index_of(self, field: Field) -> u16 {
        let span = match self {
            Self::Vanilla5875 => generated::VANILLA[field as usize],
            Self::Wrath12340 => generated::WRATH[field as usize],
        };
        match span {
            Some(span) => span.index,
            None => panic!("the field is not in this layout"),
        }
    }

    pub fn descriptor_len(self, object_type: ObjectType) -> u16 {
        let slot = Self::class_slot(object_type);
        match self {
            Self::Vanilla5875 => generated::VANILLA_ENDS[slot],
            Self::Wrath12340 => generated::WRATH_ENDS[slot],
        }
    }

    #[inline]
    fn class_slot(object_type: ObjectType) -> usize {
        match object_type {
            ObjectType::Object => 0,
            ObjectType::Item => 1,
            ObjectType::Container => 2,
            ObjectType::Unit => 3,
            ObjectType::Player => 4,
            ObjectType::GameObject => 5,
            ObjectType::DynamicObject => 6,
            ObjectType::Corpse => 7,
        }
    }

    /// Constant-time reverse mapping for change watchers, including inherited blocks.
    /// Object class disambiguates overlapping blocks; the offset is a native dword offset.
    #[inline]
    pub fn field_at(self, object_type: ObjectType, index: u16) -> Option<(Field, u16)> {
        self.reverse_table(object_type)
            .get(usize::from(index))
            .copied()
            .flatten()
    }

    #[inline]
    fn reverse_table(self, object_type: ObjectType) -> &'static [Option<(Field, u16)>] {
        let slot = Self::class_slot(object_type);
        match self {
            Self::Vanilla5875 => generated::VANILLA_REVERSE[slot],
            Self::Wrath12340 => generated::WRATH_REVERSE[slot],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reverse_lookup_is_a_direct_table_index_and_matches_forward_spans() {
        for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
            for class in [
                ObjectType::Object,
                ObjectType::Item,
                ObjectType::Container,
                ObjectType::Unit,
                ObjectType::Player,
                ObjectType::GameObject,
                ObjectType::DynamicObject,
                ObjectType::Corpse,
            ] {
                let table = layout.reverse_table(class);
                assert_eq!(table.len(), usize::from(layout.descriptor_len(class)));
                for index in 0..layout.descriptor_len(class) {
                    let expected = Field::ALL.into_iter().find_map(|field| {
                        let span = layout.resolve(field)?;
                        let inherited = span.object_type == ObjectType::Object
                            || (class == ObjectType::Player
                                && span.object_type == ObjectType::Unit)
                            || (class == ObjectType::Container
                                && span.object_type == ObjectType::Item);
                        if span.object_type != class && !inherited {
                            return None;
                        }
                        let offset = index.checked_sub(span.index)?;
                        (offset < span.width).then_some((field, offset))
                    });
                    assert_eq!(layout.field_at(class, index), table[usize::from(index)]);
                    assert_eq!(
                        layout.field_at(class, index),
                        expected,
                        "{layout:?} {class:?} {index}"
                    );
                }
                assert_eq!(layout.field_at(class, layout.descriptor_len(class)), None);
                assert_eq!(layout.field_at(class, u16::MAX), None);
            }
        }
    }

    #[test]
    fn semantic_fields_resolve_native_indices_and_widths() {
        for (layout, health, quest, quest_width, player_end) in [
            (FieldLayout::Vanilla5875, 22, 198, 60, 1282),
            (FieldLayout::Wrath12340, 24, 158, 125, 1326),
        ] {
            assert_eq!(layout.resolve(Field::UnitHealth).unwrap().index, health);
            let span = layout.resolve(Field::PlayerQuestLog).unwrap();
            assert_eq!((span.index, span.width), (quest, quest_width));
            assert_eq!(layout.descriptor_len(ObjectType::Player), player_end);
            assert_eq!(
                layout.field_at(ObjectType::Player, quest + quest_width - 1),
                Some((Field::PlayerQuestLog, quest_width - 1))
            );
        }
        assert!(FieldLayout::Wrath12340.resolve(Field::UnitAura).is_none());
        assert!(FieldLayout::Vanilla5875
            .resolve(Field::UnitFlags2)
            .is_none());
    }
    #[test]
    fn reverse_lookup_disambiguates_object_classes() {
        let layout = FieldLayout::Vanilla5875;
        assert_eq!(
            layout.field_at(ObjectType::Unit, 36),
            Some((Field::UnitBytes0, 0))
        );
        assert_eq!(
            layout.field_at(ObjectType::Corpse, 36),
            Some((Field::CorpseDynamicFlags, 0))
        );
    }
    #[test]
    fn grouped_fields_do_not_swallow_following_wrath_fields() {
        let layout = FieldLayout::Wrath12340;
        let visible = layout.resolve(Field::PlayerVisibleItems).unwrap();
        assert_eq!(visible.width, 38);
        let title = layout.resolve(Field::PlayerChosenTitle).unwrap();
        assert_eq!(visible.index + visible.width, title.index);
        assert_eq!(
            layout.field_at(ObjectType::Player, title.index),
            Some((Field::PlayerChosenTitle, 0))
        );
    }

    #[test]
    fn every_span_fits_its_native_descriptor() {
        for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
            for field in Field::ALL {
                if let Some(span) = layout.resolve(field) {
                    assert!(span.width > 0, "{layout:?} {field:?}");
                    assert!(
                        span.index + span.width <= layout.descriptor_len(span.object_type),
                        "{layout:?} {field:?} {span:?}"
                    );
                }
            }
        }
    }
}
