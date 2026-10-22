//! The stock glue's realm list (`crate::ui_script::glue`): the native list's state behind
//! `GetRealmInfo`, `ChangeRealm`, `RealmListDialogCancelled` and `RequestRealmList`.

use super::{is_down, is_invalid, load, Realms, RealmsCategory, RealmsSelect, REFRESH_SECS};
use crate::net::{RealmChoice, RealmRequest};

/// The refresh period `RealmListUpdateRate` answers: this list's own.
pub(crate) const GLUE_UPDATE_RATE: f32 = REFRESH_SECS;

/// Every tab by ordinal (1..), each tab's rows in the list's order, as the stock `GetRealmInfo`
/// reads them: the load band against every realm, `(pvp, rp)` by realm type, down, invalid and
/// current as the native list paints them. Labelled: `locked` reads false; the shared realm
/// record does not carry it. `realms`' front tab is restored.
pub(crate) fn glue_tabs(realms: &mut Realms) -> Vec<wrathplus_ui::script::glue::GlueRealmTab> {
    let names: Vec<String> = realms.tabs().into_iter().map(str::to_string).collect();
    let (mean, stddev) = realms.stats();
    let front = realms.category;
    let mut out = Vec::new();
    for ordinal in 1..=names.len().max(1) {
        realms.category = Some(ordinal);
        let rows = realms
            .rows()
            .into_iter()
            .map(|i| {
                let r = &realms.realms[i];
                let (pvp, rp) = load::pvp_rp(r.realm_type);
                wrathplus_ui::script::glue::GlueRealm {
                    name: r.name.clone(),
                    num_characters: u32::from(r.characters),
                    invalid: is_invalid(r),
                    down: is_down(r),
                    current: realms
                        .current
                        .as_deref()
                        .is_some_and(|c| c.eq_ignore_ascii_case(&r.name)),
                    pvp,
                    rp,
                    load: load::realm_load_classify(r.flags, r.population, mean, stddev),
                    locked: false,
                    realm_type: r.realm_type,
                }
            })
            .collect();
        out.push(wrathplus_ui::script::glue::GlueRealmTab {
            name: names.get(ordinal - 1).cloned().unwrap_or_default(),
            realms: rows,
        });
    }
    realms.category = front;
    out
}

/// The stock `ChangeRealm(category, index)`: the `index`-th row (1-based) of tab `category`
/// answers the park, as the native OK does; a down realm or a row past the end does nothing.
pub(crate) fn glue_change_realm(
    realms: &mut Realms,
    choice: &RealmChoice,
    category: usize,
    index: usize,
) {
    realms.category = Some(category.max(1));
    let rows = realms.rows();
    let Some(realm) = index
        .checked_sub(1)
        .and_then(|i| rows.get(i))
        .map(|&i| &realms.realms[i])
    else {
        return;
    };
    if is_down(realm) {
        return;
    }
    let name = realm.name.clone();
    realms.hide();
    realms.store_front_category();
    realms.enter(choice, name);
}

/// The stock `RealmListDialogCancelled`: the native Cancel, which abandons the park.
pub(crate) fn glue_cancel(realms: &mut Realms, choice: &RealmChoice) {
    realms.hide();
    let _ = choice.0.send(RealmRequest::Abandon);
}

/// The stock `RequestRealmList`: the refresh.
pub(crate) fn glue_refresh(choice: &RealmChoice) {
    let _ = choice.0.send(RealmRequest::Refresh);
}
