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

/// Build 12340's descriptor table, the one layout: a semantic field's span, its first dword, a
/// class's descriptor length and the reverse map change watchers read.
impl Field {
    /// The field's dword span.
    #[inline]
    pub fn span(self) -> Option<FieldSpan> {
        generated::WRATH[self as usize]
    }

    /// The field's first dword index, for fixtures and watch constants that still key on an
    /// index. A field the table lacks is a compile-time error in a `const`.
    pub const fn index(self) -> u16 {
        match generated::WRATH[self as usize] {
            Some(span) => span.index,
            None => panic!("the field is not in the 12340 layout"),
        }
    }

    /// The descriptor length of `object_type`, below which a CREATE's absent field reads 0.
    pub fn descriptor_len(object_type: ObjectType) -> u16 {
        generated::WRATH_ENDS[class_slot(object_type)]
    }

    /// Constant-time reverse mapping for change watchers, including inherited blocks.
    /// Object class disambiguates overlapping blocks; the offset is a native dword offset.
    #[inline]
    pub fn at(object_type: ObjectType, index: u16) -> Option<(Field, u16)> {
        generated::WRATH_REVERSE[class_slot(object_type)]
            .get(usize::from(index))
            .copied()
            .flatten()
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reverse_lookup_is_a_direct_table_index_and_matches_forward_spans() {
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
            let table = generated::WRATH_REVERSE[class_slot(class)];
            assert_eq!(table.len(), usize::from(Field::descriptor_len(class)));
            for index in 0..Field::descriptor_len(class) {
                let expected = Field::ALL.into_iter().find_map(|field| {
                    let span = field.span()?;
                    let inherited = span.object_type == ObjectType::Object
                        || (class == ObjectType::Player && span.object_type == ObjectType::Unit)
                        || (class == ObjectType::Container && span.object_type == ObjectType::Item);
                    if span.object_type != class && !inherited {
                        return None;
                    }
                    let offset = index.checked_sub(span.index)?;
                    (offset < span.width).then_some((field, offset))
                });
                assert_eq!(Field::at(class, index), table[usize::from(index)]);
                assert_eq!(
                    Field::at(class, index),
                    expected,
                    "{class:?} {index}"
                );
            }
            assert_eq!(Field::at(class, Field::descriptor_len(class)), None);
            assert_eq!(Field::at(class, u16::MAX), None);
        }
    }

    #[test]
    fn semantic_fields_resolve_native_indices_and_widths() {
        let (health, quest, quest_width, player_end) = (24, 158, 125, 1326);
        assert_eq!(Field::UnitHealth.span().unwrap().index, health);
        let span = Field::PlayerQuestLog.span().unwrap();
        assert_eq!((span.index, span.width), (quest, quest_width));
        assert_eq!(Field::descriptor_len(ObjectType::Player), player_end);
        assert_eq!(
            Field::at(ObjectType::Player, quest + quest_width - 1),
            Some((Field::PlayerQuestLog, quest_width - 1))
        );
    }
    #[test]
    fn reverse_lookup_disambiguates_object_classes() {
        assert_eq!(
            Field::at(ObjectType::Unit, 23),
            Some((Field::UnitBytes0, 0))
        );
        assert_eq!(
            Field::at(ObjectType::Corpse, 23),
            Some((Field::CorpseItem, 12))
        );
    }
    #[test]
    fn grouped_fields_do_not_swallow_following_wrath_fields() {
        let visible = Field::PlayerVisibleItems.span().unwrap();
        assert_eq!(visible.width, 38);
        let title = Field::PlayerChosenTitle.span().unwrap();
        assert_eq!(visible.index + visible.width, title.index);
        assert_eq!(
            Field::at(ObjectType::Player, title.index),
            Some((Field::PlayerChosenTitle, 0))
        );
    }

    #[test]
    fn every_span_fits_its_native_descriptor() {
        for field in Field::ALL {
            if let Some(span) = field.span() {
                assert!(span.width > 0, "{field:?}");
                assert!(
                    span.index + span.width <= Field::descriptor_len(span.object_type),
                    "{field:?} {span:?}"
                );
            }
        }
    }
}
