//! The price discount the client takes off the two prices it computes from its own DBCs: the repair cost (`0x4faf30`, caller `0x4faf8a`) and the
//! taxi map's fare (`0x4dc3d0`, callers `0x4dc45d` and `0x4dc4a2`). The merchant's goods and the
//! trainer's services are the server's prices, shown as sent.

use crate::net::{ObjectStore, Reputations};

use super::{ring_reaction, Factions};

/// Wrath: 3.3.5 has no honor ranks (`PLAYER_BYTES_3` byte 2 is the PvP title), so only the
/// reputation term applies, 5% per rank above Neutral, as AzerothCore charges it
/// (`Player::GetReputationPriceDiscount`, used for repairs and taxi fares). The 12340 client's own
/// routine is untraced; the vendor's reaction toward the player stands in for the rank, and its
/// Exalted rung depends on that reaction reaching 7.
fn wrath_price_discount(reaction: u8) -> f64 {
    const NEUTRAL: u8 = 3;
    0.05 * f64::from(reaction.saturating_sub(NEUTRAL))
}

/// [`wrath_price_discount`] for `vendor` and the player: the vendor's reaction toward the player
/// ([`ring_reaction`]).
pub(crate) fn vendor_price_discount(
    factions: Option<&Factions>,
    reputations: &Reputations,
    vendor: &ObjectStore,
    player: &ObjectStore,
) -> f64 {
    wrath_price_discount(ring_reaction(
        factions,
        reputations,
        Some(vendor),
        Some(player),
    ))
}

/// A fixture off the install's `Faction.dbc`: the catalog, a `FactionTemplate` of Stormwind
/// (faction 72, which has a reputation slot) and a reputation table putting a human warrior
/// (`UNIT_FIELD_BYTES_0` = [`HUMAN_WARRIOR`]) at `total` standing with it.
#[cfg(test)]
pub(crate) fn stormwind_fixture(
    chain: &mut benilla_formats::Chain,
    total: i32,
) -> (Factions, u32, Reputations) {
    const STORMWIND: u32 = 72;
    let catalog = benilla_formats::load_faction_catalog(chain).expect("Faction.dbc");
    let template = (1u32..4096)
        .find(|&id| catalog.template(id).is_some_and(|t| t.faction == STORMWIND))
        .expect("a Stormwind template");
    let info = catalog
        .reputation_faction(STORMWIND)
        .expect("Stormwind has a reputation slot");
    let slot = usize::try_from(info.rep_index).expect("a slot index");
    let mut reps = vec![(0u8, 0i32); slot + 1];
    reps[slot].1 = total - info.base_for(1, 1);
    (Factions::from_catalog(catalog), template, Reputations(reps))
}

/// `UNIT_FIELD_BYTES_0` of a human (race 1) warrior (class 1), for [`stormwind_fixture`].
#[cfg(test)]
pub(crate) const HUMAN_WARRIOR: u32 = 1 | (1 << 8);

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::messages::{Field, FieldLayout};
    use benilla_protocol::ObjectFields;

    const FIELD_UNIT_FACTIONTEMPLATE: u16 =
        FieldLayout::Wrath12340.index_of(Field::UnitFactionTemplate);
    /// `UNIT_FIELD_BYTES_0`, absolute descriptor index.
    const BYTES_0: u16 = FieldLayout::Wrath12340.index_of(Field::UnitBytes0);

    /// Wrath: 5% per rank above Neutral, never the 1.12 honor steps.
    #[test]
    fn the_wrath_discount_is_the_reputation_ladder_only() {
        let d = super::wrath_price_discount;
        assert_eq!(d(3), 0.0);
        assert_eq!(d(1), 0.0);
        assert!((d(4) - 0.05).abs() < 1e-12);
        assert!((d(5) - 0.10).abs() < 1e-12);
        assert!((d(7) - 0.20).abs() < 1e-12);
    }

    /// The reputation term reads the vendor's reaction to the player, the standing rank: Friendly
    /// (8999) earns 5%, Honored (9000) 10%.
    #[test]
    fn the_reputation_term_is_the_vendors_reaction_to_the_player() {
        let data = benilla_formats::wow_data_or_skip!();
        let mut chain = benilla_formats::open_chain(&data).expect("open chain");
        for (total, want) in [(8999, 0.05), (9000, 0.10)] {
            let (factions, template, reps) = stormwind_fixture(&mut chain, total);
            let vendor = ObjectStore(ObjectFields::from_pairs(&[(
                FIELD_UNIT_FACTIONTEMPLATE,
                template,
            )]));
            let me = ObjectStore(ObjectFields::from_pairs(&[(BYTES_0, HUMAN_WARRIOR)]));
            let got = vendor_price_discount(Some(&factions), &reps, &vendor, &me);
            assert!((got - want).abs() < 1e-12, "standing {total}: got {got:e}");
        }
    }
}
