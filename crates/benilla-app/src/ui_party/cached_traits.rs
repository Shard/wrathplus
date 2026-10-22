//! An unstreamed group member's race, class and sex, off the name cache.

use benilla_ui::script::UnitState;

/// An unstreamed member's race, class and sex off the name cache's answer for their guid, the
/// fallback the getters take with no object: the roster and the stats record carry none of the three.
/// Nothing changes without an answer, so `UnitClass` stays nil until the query lands.
pub(super) fn fill(s: &mut UnitState, traits: Option<(u8, u8, u8)>) {
    let Some((race, class, gender)) = traits else {
        return;
    };
    let race = crate::app_core::catalogs::race_names(race);
    let class = crate::app_core::catalogs::class_names(class);
    s.race = race.map(|(n, _)| n.to_string());
    s.race_file = race.map(|(_, f)| f.to_string());
    s.class = class.map(|(n, _)| n.to_string());
    s.class_file = class.map(|(_, f)| f.to_string());
    // `UnitSex`'s scale, as the live leg maps the gender byte: 2 male, 3 female, 0 unknown.
    s.sex = match gender {
        0 => 2,
        1 => 3,
        _ => 0,
    };
}
