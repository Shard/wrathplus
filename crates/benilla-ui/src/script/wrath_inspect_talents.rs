//! Build-12340 inspected talents: the host's reduction of `SMSG_INSPECT_TALENT` (`0x3F4`,
//! `docs/WRATH-INSPECT-TALENTS.md`). The host pushes the latest reply for the inspected player,
//! then fires `INSPECT_TALENT_READY`; the talent getters' inspect branches read it.

/// One talent group (spec) as the inspected player has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathInspectGroup {
    /// The class's pages with the inspected ranks; `points` is `(unspent, 0)`, and no talent is
    /// learnable.
    pub pages: super::TalentUiState,
    /// `GlyphProperties.dbc` ids per glyph socket, 0 for an empty socket.
    pub glyphs: Vec<u16>,
}

/// The inspected player's talents.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathInspectTalents {
    pub guid: u64,
    /// Unspent talent points.
    pub unspent: u32,
    /// The active group, 1-based, as `GetActiveTalentGroup(true)` answers.
    pub active_group: u32,
    /// Up to two groups; empty when the realm disables talent inspection.
    pub groups: Vec<WrathInspectGroup>,
}

impl super::UiScript {
    /// Replace the inspected talents; `None` once nothing is inspected. The host fires
    /// `INSPECT_TALENT_READY` after a reply.
    pub fn set_wrath_inspect_talents(&mut self, talents: Option<WrathInspectTalents>) {
        self.model_mut().wrath_inspect_talents = talents;
    }

    /// The pushed inspected talents.
    pub fn wrath_inspect_talents(&self) -> Option<WrathInspectTalents> {
        self.model_ref().wrath_inspect_talents.clone()
    }
}
