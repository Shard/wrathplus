//! L1 input boundary. Raw receipts and derived presentation have independent ownership.
//! No protocol imports, implicit notifications, request sends or completion rewards.
use std::{collections::HashMap, sync::Arc};

use super::UiScript;

/// A full packed dungeon entry and its raw server lock reason (including zero).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgLock {
    pub entry: u32,
    pub reason: u32,
}

/// One preview item in receipt order; `display` is not necessarily its template display.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgRewardItem {
    pub item: u32,
    pub display: u32,
    pub count: u32,
}

/// Random reward preview, not the separate L4 completion reward.
/// Native PLAYER_INFO stores the four words at +14/+18/+1c/+20;
/// GetLFGDungeonRewards (0x557aa0) presents them in base/variance pairs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgPreview {
    pub entry: u32,
    pub done: u8,
    pub money_base: u32,
    pub xp_base: u32,
    pub money_var: u32,
    pub xp_var: u32,
    pub items: Vec<WrathLfgRewardItem>,
}

/// Original player receipt vectors. Do not deduplicate or merge them in the feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgPlayerLocks {
    pub locks: Vec<WrathLfgLock>,
    pub previews: Vec<WrathLfgPreview>,
}

/// Original party record, retaining full GUID and raw lock order/duplicates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgMemberLocks {
    pub guid: u64,
    pub locks: Vec<WrathLfgLock>,
}

/// Independent validity: `None` is invalid, while `Some(Default::default())` is
/// a valid empty receipt. Requests and item readiness must not invalidate either.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgLockSnapshot {
    pub player: Option<WrathLfgPlayerLocks>,
    pub party: Option<Vec<WrathLfgMemberLocks>>,
}

/// Derived inputs, never server cache validity. Missing keys mean unavailable;
/// an explicitly empty name is retained as resolved-but-unknown.
/// Item names/quality already belong to the shared item-template store; static
/// dungeon metadata and expansion context remain in the L0 catalog.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgPresentation {
    pub self_guid: u64,
    pub guid_names: HashMap<u64, String>,
    pub display_icons: HashMap<u32, String>,
    /// Native nine-field links, built by App from cached templates and semantic
    /// player level (0 when no self object). Not the Vanilla item-link helper.
    pub reward_links: HashMap<u32, String>,
}

impl UiScript {
    /// Publish current authoritative caches without events or request side effects.
    /// App replaces only the relevant optional block in its sole reducer, then
    /// installs each receipt before its synchronous notification pair. VM reseeding
    /// uses this same setter without replaying old receipts.
    /// The native random list and best choice are rebuilt here (`0x55aaf0` runs on each receipt).
    pub fn set_wrath_lfg_lock_snapshot(&mut self, snapshot: Arc<WrathLfgLockSnapshot>) {
        let mut model = self.model_mut();
        model.wrath_lfg.locks = snapshot;
        model.wrath_lfg.rederive();
    }

    /// Publish derived readiness before notifications, independently of lock validity.
    /// This setter does not manufacture the receipt pair or an item callback event.
    pub fn set_wrath_lfg_presentation(&mut self, presentation: Arc<WrathLfgPresentation>) {
        self.model_mut().wrath_lfg.presentation = presentation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::UiScript;

    #[test]
    fn lock_inputs_preserve_valid_empty_raw_duplicates_and_packed_types() {
        let mut ui = UiScript::new().unwrap();
        assert_eq!(
            *ui.model_mut().wrath_lfg.locks,
            WrathLfgLockSnapshot::default()
        );
        let locks = vec![
            WrathLfgLock {
                entry: 0x06000007,
                reason: 0,
            },
            WrathLfgLock {
                entry: 0x06000007,
                reason: u32::MAX,
            },
            WrathLfgLock {
                entry: 0x02000007,
                reason: 5,
            },
        ];
        let snapshot = Arc::new(WrathLfgLockSnapshot {
            player: Some(WrathLfgPlayerLocks {
                locks,
                previews: vec![WrathLfgPreview {
                    entry: 0x06000007,
                    done: 255,
                    money_base: u32::MAX,
                    money_var: 1,
                    xp_base: 2,
                    xp_var: 3,
                    items: vec![WrathLfgRewardItem {
                        item: 4,
                        display: 5,
                        count: u32::MAX,
                    }],
                }],
            }),
            party: Some(vec![]),
        });
        ui.set_wrath_lfg_lock_snapshot(snapshot.clone());
        assert!(Arc::ptr_eq(&ui.model_mut().wrath_lfg.locks, &snapshot));
        ui.set_wrath_lfg_presentation(Arc::new(WrathLfgPresentation::default()));
        assert_eq!(ui.model_mut().wrath_lfg.locks.as_ref(), snapshot.as_ref());
        ui.set_wrath_lfg_lock_snapshot(Arc::new(WrathLfgLockSnapshot {
            player: None,
            party: Some(vec![WrathLfgMemberLocks {
                guid: u64::MAX,
                locks: vec![],
            }]),
        }));
        assert_eq!(
            ui.model_mut().wrath_lfg.locks.party.as_ref().unwrap()[0].guid,
            u64::MAX
        );
        assert!(ui.take_cvar_changes().is_empty());
        assert!(ui.errors().is_empty());
    }

    #[test]
    fn replacement_seeds_shared_inputs_without_receipt_notifications() {
        let snapshot = Arc::new(WrathLfgLockSnapshot {
            player: Some(WrathLfgPlayerLocks::default()),
            party: None,
        });
        let mut presentation = WrathLfgPresentation {
            self_guid: u64::MAX,
            ..Default::default()
        };
        presentation.guid_names.insert(u64::MAX, String::new());
        presentation
            .display_icons
            .insert(5, "Interface\\Icons\\INV_Misc_QuestionMark".into());
        let presentation = Arc::new(presentation);
        for _ in 0..2 {
            let mut ui = UiScript::new().unwrap();
            ui.run("received=0; local f=CreateFrame('Frame'); f:RegisterEvent('LFG_LOCK_INFO_RECEIVED'); f:RegisterEvent('LFG_UPDATE_RANDOM_INFO'); f:SetScript('OnEvent',function() received=received+1 end)").unwrap();
            ui.set_wrath_lfg_lock_snapshot(snapshot.clone());
            ui.set_wrath_lfg_presentation(presentation.clone());
            assert!(Arc::ptr_eq(&ui.model_mut().wrath_lfg.locks, &snapshot));
            assert!(Arc::ptr_eq(
                &ui.model_mut().wrath_lfg.presentation,
                &presentation
            ));
            assert_eq!(ui.eval::<i64>("return received").unwrap(), 0);
            assert!(ui
                .model_mut()
                .wrath_lfg
                .presentation
                .guid_names
                .contains_key(&u64::MAX));
            assert!(ui
                .model_mut()
                .wrath_lfg
                .presentation
                .reward_links
                .is_empty());
        }
    }
}
