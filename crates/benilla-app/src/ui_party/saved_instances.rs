//! Native saved-instance receipt ownership. Receiver 0x501030 installs every list before
//! UPDATE_INSTANCE_INFO, including duplicates and empty answers. UI projection is a later seam;
//! keep ordered receipts as well as the latest snapshot so a feed cannot coalesce packet edges.
use benilla_protocol::wrath_instance::RaidInstanceInfo;

pub(crate) use crate::app_core::group::NativeSavedInstances;
impl NativeSavedInstances {
    pub(super) fn receive(&mut self, info: RaidInstanceInfo) {
        self.snapshot = Some(info.clone());
        self.receipts.push(info);
    }
}

