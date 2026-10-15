//! Raw group metadata for the native LFD/LFR choice builders (0x558350/0x558520).
use std::sync::Arc;

use super::UiScript;

/// Localized LFGDungeonGroup runtime row; negative choice IDs reference `-id`.
/// Keep the raw header word even though the native choice builder does not read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgGroup {
    pub id: u32,
    pub name: String,
    pub sort_priority: i32,
    pub raw_header_word: u32,
    pub dungeon_type: i32,
}

impl UiScript {
    /// Replace group metadata without replacing dungeon metadata or session context.
    /// App owns restoration across VM replacement. Source order and duplicate rows survive.
    pub fn set_wrath_lfg_groups(&mut self, groups: Arc<Vec<WrathLfgGroup>>) {
        self.model_mut().wrath_lfg.groups = groups;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{ScriptDialect, WrathLfgCatalog};

    #[test]
    fn independent_metadata_setters_preserve_shared_inputs() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let groups = Arc::new(vec![
            WrathLfgGroup {
                id: 4,
                name: "First".into(),
                sort_priority: -3,
                raw_header_word: u32::MAX,
                dungeon_type: -1,
            },
            WrathLfgGroup {
                id: 4,
                name: "Duplicate".into(),
                ..Default::default()
            },
        ]);
        let catalog = Arc::new(WrathLfgCatalog::default());
        script.set_wrath_lfg_groups(groups.clone());
        script.set_wrath_lfg_catalog(catalog.clone(), Some(0));
        {
            let model = script.model_mut();
            assert!(Arc::ptr_eq(&model.wrath_lfg.groups, &groups));
            assert!(Arc::ptr_eq(&model.wrath_lfg.catalog, &catalog));
            assert_eq!(model.wrath_lfg.expansion, Some(0));
            assert_eq!(model.wrath_lfg.groups.as_ref(), groups.as_ref());
        }
        script.set_wrath_lfg_groups(Arc::new(Vec::new()));
        let model = script.model_mut();
        assert!(Arc::ptr_eq(&model.wrath_lfg.catalog, &catalog));
        assert_eq!(model.wrath_lfg.expansion, Some(0));
        assert!(model.wrath_lfg.groups.is_empty());
    }
}
