//! The tradeskill bindings behind the stock `Blizzard_TradeSkillUI`: the app pushes a snapshot
//! ([`UiScript::set_trade_skill`]) with recipes resolved from `Spell.dbc`, `SkillLineAbility.dbc`
//! and bag counts, and drains the intents `DoTradeSkill` and `CloseTradeSkill` queue. The engine
//! owns the grouped, filtered list (`0x4fca20`); every Lua index, `SetTradeSkillItem`'s included,
//! is 1-based into its visible rows ([`recipe_at`]).

use std::collections::HashSet;

mod api;
mod view;

#[cfg(test)]
mod tests;

use view::build_groups;

pub(super) use api::{install, install_wrath};
pub(crate) use view::recipe_at;

/// A recipe's difficulty band, computed app-side: gray at or above trivialHigh, green from the
/// low/high midpoint, yellow from trivialLow, orange below (`0x4fca20`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeSkillDifficulty {
    /// Orange: a near-certain skill-up.
    Optimal,
    /// Yellow: a good skill-up chance.
    Medium,
    /// Green: a fading skill-up chance.
    Easy,
    /// Gray: no skill-up.
    Trivial,
}

impl TradeSkillDifficulty {
    /// A recipe row's `type` in `GetTradeSkillInfo`, a `TradeSkillTypeColor` key
    /// (`Blizzard_TradeSkillUI.lua:7-10`).
    pub fn as_str(self) -> &'static str {
        match self {
            TradeSkillDifficulty::Optimal => "optimal",
            TradeSkillDifficulty::Medium => "medium",
            TradeSkillDifficulty::Easy => "easy",
            TradeSkillDifficulty::Trivial => "trivial",
        }
    }

    /// The tier byte recipes sort by, ascending (`0x4fd380`).
    pub(crate) fn tier(self) -> u8 {
        match self {
            TradeSkillDifficulty::Optimal => 0,
            TradeSkillDifficulty::Medium => 1,
            TradeSkillDifficulty::Easy => 2,
            TradeSkillDifficulty::Trivial => 3,
        }
    }
}

/// One reagent of a recipe, as `GetTradeSkillReagentInfo` reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct TradeSkillReagent {
    /// The item entry `SetTradeSkillItem(i, j)` resolves through.
    pub item: u32,
    /// `None` until the item template arrives.
    pub name: Option<String>,
    /// `None` until the item template arrives.
    pub icon: Option<String>,
    /// How many this recipe consumes.
    pub need: u32,
    /// How many the player's bags hold.
    pub have: u32,
}

/// One recipe: a known spell with `SPELL_ATTR_IS_TRADESKILL`, resolved app-side from `Spell.dbc`,
/// `SkillLineAbility.dbc` and bag counts.
#[derive(Clone, Debug, PartialEq)]
pub struct TradeSkillRecipe {
    /// The product's `(ItemClass, ItemSubClass)` and `ItemSubClass.dbc` name, the header key
    /// (`0x55ba30`); `None` until the product's template arrives.
    pub group: Option<(u32, u32, String)>,
    /// What `DoTradeSkill` casts, one `CMSG_CAST_SPELL` per item: the packet has no count.
    pub spell_id: u32,
    pub name: String,
    pub difficulty: TradeSkillDifficulty,
    /// The least `have / need` over the reagents, reported as `numAvailable`.
    pub num_available: u32,
    /// The product's icon, else the spell's; `None` while neither has arrived.
    pub icon: Option<String>,
    pub min_made: u32,
    pub max_made: u32,
    /// Remaining cooldown in seconds; `None` when ready.
    pub cooldown_secs: Option<u64>,
    /// The created item, 0 for a pure-effect recipe.
    pub product_item: u32,
    /// The product's `InventoryType`, which the inv-slot filter folds to slot bits through
    /// `DAT_00809200` (`0x4fcee6`); 0 takes the catch-all bit.
    pub product_inv_type: u32,
    /// The product's `ItemLevel`, the sort key between tier and name (`record+0x14`, `0x7c9640`).
    pub product_item_level: u32,
    /// In display order.
    pub reagents: Vec<TradeSkillReagent>,
    /// The Requirements line's `(tool name, have)` pairs.
    pub tools: Vec<(String, bool)>,
    /// Build 12340's `GetTradeSkillDescription`: the spell's description, tokens filled; `None`
    /// for an empty one.
    pub description: Option<String>,
    /// `GetTradeSkillInfo`'s fifth return (`0x5db2a0`): the open line's `alternateVerb`, empty
    /// included, for a spell whose first effect creates no item (not 24, 157 or 59); `None`
    /// otherwise.
    pub alt_verb: Option<String>,
}

/// One open tradeskill window, pushed whole by the app.
#[derive(Clone, Debug, PartialEq)]
pub struct TradeSkillState {
    /// The `SkillLine.dbc` id, the persistence key (`0xbde064`).
    pub line: u32,
    pub line_name: String,
    pub rank: u32,
    pub max_rank: u32,
    /// Unordered: the engine sorts them (`0x4fca20`), and Lua indexes the visible rows, not this.
    pub recipes: Vec<TradeSkillRecipe>,
    /// Remaining Create All repeats (`0x500230`), decremented by the app as each cast resolves.
    pub repeat_count: u32,
    /// The linked window's owner (`IsTradeSkillLinked`, `0x5da310`): their name, empty until the
    /// name cache answers; `None` for our own book.
    pub linked: Option<String>,
    /// `GetTradeSkillListLink` (`0x5dc0b0`): our own trade link for the open line's opener, `None`
    /// when the line cannot link or we lack the skill.
    pub list_link: Option<String>,
}

impl super::UiScript {
    /// Pushes the open window's snapshot, or `None` on close; the app re-pushes on any change.
    ///
    /// Persistence follows the reference (`0x4fc910`, keyed by `0xbde064`): a different skill line
    /// resets filters, folds and selection, and the same line keeps them across a close. The
    /// selection rides its spell id (`0xbde044`); fold and subclass keys are pruned to live groups,
    /// while the inv-slot mask (`0x84dd64`) is never pruned.
    pub fn set_trade_skill(&mut self, state: Option<TradeSkillState>) {
        let mut model = self.model_mut();
        match state {
            None => {
                model.trade_skill_selection = 0;
                model.trade_skill = None;
            }
            Some(s) => {
                if model.trade_skill_last_line != s.line {
                    model.trade_skill_collapsed.clear();
                    model.trade_skill_subclass_hidden.clear();
                    model.trade_skill_invslot_mask = u32::MAX;
                    model.trade_skill_selected_spell = 0;
                    model.trade_skill_last_line = s.line;
                }
                model.trade_skill_selection = (model.trade_skill_selected_spell != 0)
                    .then(|| {
                        s.recipes
                            .iter()
                            .position(|r| r.spell_id == model.trade_skill_selected_spell)
                    })
                    .flatten()
                    .map_or(0, |i| (i + 1) as u32);
                let live: HashSet<(u32, u32)> = build_groups(&s.recipes)
                    .into_iter()
                    .map(|g| g.key)
                    .collect();
                model.trade_skill_collapsed.retain(|k| live.contains(k));
                model
                    .trade_skill_subclass_hidden
                    .retain(|k| live.contains(k));
                model.trade_skill = Some(s);
            }
        }
    }

    /// Drains the `(spell id, count)` crafts `DoTradeSkill` queued; the app's repeat loop
    /// (`0x4fd7b0`) sends one cast per item.
    pub fn take_trade_skill_dos(&mut self) -> Vec<(u32, u32)> {
        std::mem::take(&mut self.model_mut().trade_skill_dos)
    }

    /// Whether `CloseTradeSkill` ran since the last drain; the close sends no packet.
    pub fn take_trade_skill_close(&mut self) -> bool {
        std::mem::take(&mut self.model_mut().trade_skill_close)
    }

    /// Whether a filter or fold changed since the last drain. The reference re-sorts and fires
    /// `TRADE_SKILL_UPDATE` (event `0x13a`) inside the call (`0x4fd710`, `0x4fd730`, `0x4fd750`);
    /// the app fires it the same frame, and the stock filter and collapse-all clicks repaint only
    /// off that event.
    pub fn take_trade_skill_touched(&mut self) -> bool {
        std::mem::take(&mut self.model_mut().trade_skill_touched)
    }

    /// Whether the name filter changed since the last drain: build 12340 fires
    /// `TRADE_SKILL_FILTER_UPDATE` (event `0x27d`) for it (`0x5dcf60`).
    pub fn take_trade_skill_filter_touched(&mut self) -> bool {
        std::mem::take(&mut self.model_mut().trade_skill_filter_touched)
    }

    /// Whether `StopTradeSkillRepeat` ran since the last drain: the Create All loop stops.
    pub fn take_trade_skill_stop_repeat(&mut self) -> bool {
        std::mem::take(&mut self.model_mut().trade_skill_stop_repeat)
    }

    /// The trade links clicked since the last drain, each a raw `trade:…` or escaped link; the app
    /// opens the linked window (`0x5de120`).
    pub fn take_trade_link_clicks(&mut self) -> Vec<String> {
        std::mem::take(&mut self.model_mut().trade_link_clicks)
    }
}
