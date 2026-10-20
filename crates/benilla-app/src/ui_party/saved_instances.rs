//! Native saved-instance receipt ownership. Receiver 0x501030 installs every list before
//! UPDATE_INSTANCE_INFO, including duplicates and empty answers. UI projection is a later seam;
//! keep ordered receipts as well as the latest snapshot so a feed cannot coalesce packet edges.
use benilla_protocol::wrath_instance::RaidInstanceInfo;

#[derive(Default)]
pub(crate) struct NativeSavedInstances {
    pub(crate) snapshot: Option<RaidInstanceInfo>,
    pub(crate) receipts: Vec<RaidInstanceInfo>,
}

impl NativeSavedInstances {
    pub(super) fn receive(&mut self, info: RaidInstanceInfo) {
        self.snapshot = Some(info.clone());
        self.receipts.push(info);
    }
}
