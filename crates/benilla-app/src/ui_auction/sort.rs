//! The auction list sort, client-side since `CMSG_AUCTION_LIST_ITEMS` carries no sort field: each
//! list keeps the reference's most-recently-clicked stack of `(key, reversed)` pairs (`0x4cd940`),
//! and every key below the primary breaks ties.

use std::cmp::Ordering;

use super::AuctionRow;

// The keys are `benilla_ui::script::SORT_KEYS`; the Lua binding refuses any other.

/// Build 12340's twelve criteria per list (`0xc0e608`, 12 × `(column, reversed, set)`); past that
/// the oldest key is forgotten.
const DEPTH: usize = 12;

/// One list's sort state: the most-recently-clicked key first.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SortStack {
    entries: Vec<(String, bool)>,
}

impl SortStack {
    /// A header click: the primary flips; any other key becomes primary with the direction it had.
    pub(crate) fn click(&mut self, key: &str) {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == key) {
            if pos == 0 {
                self.entries[0].1 = !self.entries[0].1;
            } else {
                let entry = self.entries.remove(pos);
                self.entries.insert(0, entry);
            }
        } else {
            self.entries.insert(0, (key.to_string(), false));
            self.entries.truncate(DEPTH);
        }
    }

    /// `SortAuctionClearSort` (`0x59a5b0`/`0x59a660`/`0x59a710`).
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    /// `SortAuctionSetSort` (`0x59a5e0`): the key moves to the front with this direction.
    pub(crate) fn set(&mut self, key: &str, reversed: bool) {
        self.entries.retain(|(k, _)| k != key);
        self.entries.insert(0, (key.to_string(), reversed));
        self.entries.truncate(DEPTH);
    }

    /// The stack as pushed to Lua, where `IsAuctionSortReversed` reads it.
    pub(crate) fn pairs(&self) -> Vec<(String, bool)> {
        self.entries.clone()
    }

    /// Order `rows` by the whole stack, primary key first; an empty stack keeps the server's order.
    pub(crate) fn apply(&self, rows: &mut [AuctionRow]) {
        if self.entries.is_empty() {
            return;
        }
        // A stable sort: rows equal under every key keep the server's order across repaints.
        rows.sort_by(|a, b| {
            for (key, reversed) in &self.entries {
                let ord = compare_by(key, a, b);
                let ord = if *reversed { ord.reverse() } else { ord };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            Ordering::Equal
        });
    }
}

/// One key's order on a first click, build 12340's comparator `0x59b460` by column. Quality and
/// status open highest first; every other column opens lowest first. Labelled: the comparator's
/// two sides are anchored on quality opening highest first, as in 1.12; duration compares the
/// time-left bucket where the reference uses the exact deadline (after a pending-sale flag
/// AzerothCore never sets).
fn compare_by(key: &str, a: &AuctionRow, b: &AuctionRow) -> Ordering {
    // The buyout, else the effective bid: the price `buyoutthenbid` and `minbidbuyout` compare.
    let price = |r: &AuctionRow| {
        if r.buyout != 0 {
            r.buyout
        } else {
            r.displayed_bid()
        }
    };
    // Status: our own bid, then anyone's, then none.
    let status = |r: &AuctionRow| {
        if r.player_holds_bid {
            2
        } else {
            u8::from(r.current_bid != 0)
        }
    };
    match key {
        "level" => a.level.cmp(&b.level),
        "quality" => b.quality.cmp(&a.quality),
        "buyoutthenbid" => price(a)
            .cmp(&price(b))
            .then_with(|| a.displayed_bid().cmp(&b.displayed_bid())),
        "duration" => a.time_left.cmp(&b.time_left),
        "status" => status(b).cmp(&status(a)),
        "name" => name_of(a).cmp(name_of(b)),
        "minbidbuyout" => price(a).cmp(&price(b)),
        "seller" => owner_of(a).cmp(owner_of(b)),
        "bid" => a.displayed_bid().cmp(&b.displayed_bid()),
        "quantity" => a.count.cmp(&b.count),
        "buyout" => a.buyout.cmp(&b.buyout),
        _ => Ordering::Equal,
    }
}

/// A row whose name has not landed sorts as the empty string.
fn name_of(r: &AuctionRow) -> &str {
    r.name.as_deref().unwrap_or("")
}

fn owner_of(r: &AuctionRow) -> &str {
    r.owner.as_deref().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(level: u32, bid: u32, buyout: u32, name: &str) -> AuctionRow {
        AuctionRow {
            level,
            start_bid: bid,
            buyout,
            name: Some(name.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn a_click_promotes_but_only_the_primary_toggles() {
        let mut s = SortStack::default();

        s.click("bid");
        assert_eq!(s.pairs(), vec![("bid".into(), false)]);

        // Re-clicking the primary flips it.
        s.click("bid");
        assert_eq!(s.pairs(), vec![("bid".into(), true)]);

        // A new key takes the front; the old primary keeps its place behind it and its flag.
        s.click("quality");
        assert_eq!(
            s.pairs(),
            vec![("quality".into(), false), ("bid".into(), true)]
        );

        // Promoting a remembered key keeps its direction.
        s.click("bid");
        assert_eq!(
            s.pairs(),
            vec![("bid".into(), true), ("quality".into(), false)],
            "promotion is not a toggle"
        );
        let rev = |key: &str| s.pairs().iter().any(|(k, r)| k == key && *r);
        assert!(rev("bid"));
        assert!(!rev("quality"), "remembered, never reversed");
        assert!(!rev("seller"), "never clicked at all");
    }

    #[test]
    fn every_column_fits_the_twelve_deep_stack() {
        let mut s = SortStack::default();
        for k in benilla_ui::script::SORT_KEYS {
            s.click(k);
        }
        assert_eq!(s.pairs().len(), 11);
        s.click("level");
        assert_eq!(s.pairs().len(), 11, "re-click reorders, never grows");
        assert_eq!(s.pairs()[0].0, "level");
    }

    /// `SortAuctionSetSort` moves a key to the front with the given direction; clear empties.
    #[test]
    fn set_moves_a_key_to_the_front_and_clear_empties() {
        let mut s = SortStack::default();
        s.set("bid", false);
        s.set("quality", true);
        s.set("bid", true);
        assert_eq!(
            s.pairs(),
            vec![("bid".into(), true), ("quality".into(), true)]
        );
        s.clear();
        assert!(s.pairs().is_empty());
    }

    #[test]
    fn lower_keys_break_the_primary_s_ties() {
        let mut rows = vec![
            row(10, 500, 0, "Bravo"),
            row(10, 100, 0, "Alpha"),
            row(5, 900, 0, "Charlie"),
        ];
        let mut s = SortStack::default();
        s.click("bid"); // secondary
        s.click("level"); // primary
        s.apply(&mut rows);
        let got: Vec<_> = rows.iter().map(|r| r.name.clone().unwrap()).collect();
        assert_eq!(got, vec!["Charlie", "Alpha", "Bravo"], "level, then bid");
    }

    /// Build 12340's buyout arm opens lowest first, a no-buyout row first; `buyoutthenbid` falls
    /// back to the bid.
    #[test]
    fn buyout_opens_lowest_first_and_buyoutthenbid_falls_back_to_the_bid() {
        let mut rows = vec![
            row(1, 300, 0, "NoBuyout"),
            row(1, 0, 100, "Cheap"),
            row(1, 0, 5000, "Pricey"),
        ];
        let mut s = SortStack::default();
        s.click("buyout");
        s.apply(&mut rows);
        let got: Vec<_> = rows.iter().map(|r| r.name.clone().unwrap()).collect();
        assert_eq!(got, vec!["NoBuyout", "Cheap", "Pricey"]);
        let mut s = SortStack::default();
        s.click("buyoutthenbid");
        s.apply(&mut rows);
        let got: Vec<_> = rows.iter().map(|r| r.name.clone().unwrap()).collect();
        assert_eq!(got, vec!["Cheap", "NoBuyout", "Pricey"], "300 bid between");
    }

    /// Both open highest first in the reference's comparator (`0x4cd940`).
    #[test]
    fn quality_and_status_open_highest_and_mine_first() {
        let mut rows = vec![
            AuctionRow {
                quality: Some(1),
                player_holds_bid: false,
                name: Some("Common".into()),
                ..Default::default()
            },
            AuctionRow {
                quality: Some(4),
                player_holds_bid: true,
                name: Some("Epic".into()),
                ..Default::default()
            },
        ];
        let mut s = SortStack::default();
        s.click("quality");
        s.apply(&mut rows);
        assert_eq!(rows[0].name.as_deref(), Some("Epic"), "epics first");

        let mut s = SortStack::default();
        s.click("status");
        s.apply(&mut rows);
        assert_eq!(
            rows[0].name.as_deref(),
            Some("Epic"),
            "the auction you are already bidding in, first"
        );
    }

    #[test]
    fn an_empty_stack_leaves_the_servers_order_alone() {
        let mut rows = vec![row(9, 0, 0, "Zulu"), row(1, 0, 0, "Alpha")];
        SortStack::default().apply(&mut rows);
        let got: Vec<_> = rows.iter().map(|r| r.name.clone().unwrap()).collect();
        assert_eq!(got, vec!["Zulu", "Alpha"]);
    }
}
