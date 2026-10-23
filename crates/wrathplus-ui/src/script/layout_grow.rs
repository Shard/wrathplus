//! Growing the cached layout graph in place. A created frame, or a region taking its first
//! anchors, joins the roster as a named node, so creation keeps the ledger: a FrameXML load no
//! longer re-derives the whole graph once per `CreateFrame`.

use crate::widget::{FrameHandle, RegionHandle};

use super::{LayoutScope, Model, RegionRow, NO_NODE, REGION_TAG};

impl LayoutScope {
    /// Give every per-id array a slot for `id`; new slots hold no node and no memory.
    fn ensure_slot(&mut self, id: u32) {
        let n = id as usize + 1;
        if self.node_of.len() >= n {
            return;
        }
        self.last.resize(n, NO_NODE);
        self.now.resize(n, NO_NODE);
        self.node_of.resize(n, u32::MAX);
        self.dep_head.resize(n, u32::MAX);
        self.dirty.resize(n, false);
    }

    /// Append live frame `h` (`id`), unanchored and with no ScrollFrame override, as a node.
    fn add_frame(&mut self, h: FrameHandle, id: u32) {
        self.ensure_slot(id);
        #[allow(clippy::cast_possible_truncation)]
        let i = self.plan.len() as u32;
        self.node_of[id as usize] = i;
        self.plan.push((h, id, None));
    }

    /// Append live region `rh` (`id`) with its first anchor targets and its owner's id.
    fn add_region(&mut self, row: RegionRow, targets: &[u32], owner_id: Option<u32>) {
        self.ensure_slot(row.id);
        #[allow(clippy::cast_possible_truncation)]
        let i = self.regions.len() as u32 | REGION_TAG;
        self.node_of[row.id as usize] = i;
        for &t in targets {
            self.ensure_slot(t);
            self.edge(row.id, t);
        }
        if let Some(o) = owner_id {
            self.edge(row.id, o);
        }
        self.regions.push(row);
    }

    /// Drop region node `id`, whose outgoing edges go to `reads`, if no rect reads it: its row
    /// leaves the roster by swap, and its memory leaves `last_count`. `false` leaves it whole.
    fn remove_unread_region(&mut self, id: u32, reads: &[u32]) -> bool {
        if self.dep_head[id as usize] != u32::MAX {
            return false;
        }
        let n = self.node_of[id as usize];
        if n & REGION_TAG == 0 {
            return false;
        }
        for &t in reads {
            self.unlink_edge(t, id);
        }
        let i = (n & !REGION_TAG) as usize;
        self.regions.swap_remove(i);
        if let Some(moved) = self.regions.get(i) {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.node_of[moved.id as usize] = i as u32 | REGION_TAG;
            }
        }
        self.node_of[id as usize] = u32::MAX;
        if std::mem::replace(&mut self.last[id as usize], NO_NODE) != NO_NODE {
            self.last_count -= 1;
        }
        true
    }

    /// [`Self::commit`] for an incremental pass: only the re-hashed slots, and `last_screen` stays,
    /// since a screen move takes the full path. A node hashed for the first time, one this module
    /// added, joins `last_count`, so the next full derivation does not read an addition as a death.
    pub(super) fn commit_incremental(&mut self) {
        for &(id, h) in &self.staged {
            let slot = &mut self.last[id as usize];
            if *slot == NO_NODE {
                self.last_count += 1;
            }
            *slot = h;
        }
        self.staged.clear();
    }
}

impl Model {
    /// A frame minted by `frame_id`: with the ledger naming, it joins the cached graph as a named
    /// node (its scale and clamp synced, as a derivation would); otherwise the full touch.
    pub(crate) fn touch_layout_created_frame(&mut self, h: FrameHandle, id: u32) {
        let Some(frame) = self.arena.frame(h) else {
            return self.touch_layout();
        };
        if self.layout_touched.is_none() || self.layout_scope.has_node(id) {
            return self.touch_layout();
        }
        let (scale, clamp) = (frame.effective_scale, frame.clamped_to_screen);
        let input = self.layout_inputs.entry(h).or_default();
        input.scale = scale;
        input.clamp = clamp;
        if clamp {
            input.extent_x = self.screen.right;
            input.extent_y = self.screen.top;
        }
        self.layout_scope.add_frame(h, id);
        self.touch_layout_node(id);
    }

    /// A region minted by `region_id`: unanchored, it is no layout node, so it only needs slots
    /// for the edges a later anchor to it records; the ledger keeps naming.
    pub(crate) fn touch_layout_created_region(&mut self, id: u32) {
        if self.layout_touched.is_none() {
            return self.touch_layout();
        }
        self.layout_scope.ensure_slot(id);
        self.bump_layout_epoch();
    }

    /// A size or offset write on region `rh`: a graph node names itself, and an unanchored region
    /// is no layout node (a derivation skips it), so only the epoch moves; else the full touch.
    pub(crate) fn touch_layout_region_write(&mut self, rh: RegionHandle) {
        match self.region_to_id.get(&rh).copied() {
            Some(id) if self.layout_scope.has_node(id) => self.touch_layout_node(id),
            _ if self.layout_touched.is_some()
                && self
                    .region_data
                    .get(&rh)
                    .is_none_or(|d| d.anchors.is_empty()) =>
            {
                self.bump_layout_epoch();
            }
            _ => self.touch_layout(),
        }
    }

    /// Region `rh`'s anchors moved from the `old` targets to the ones its data now holds.
    pub(crate) fn touch_layout_region_anchors(&mut self, rh: RegionHandle, old: &[u32]) {
        let new: Vec<u32> = self
            .region_data
            .get(&rh)
            .map(|d| d.anchors.iter().map(|a| a.relative_to).collect())
            .unwrap_or_default();
        self.touch_layout_retarget_region(rh, old, &new);
    }

    /// An anchor retarget the cached graph could not patch: a live region taking its first
    /// anchors joins the roster as a named node; anything else is the full touch.
    pub(crate) fn touch_layout_first_anchors(
        &mut self,
        rh: RegionHandle,
        old: &[u32],
        new: &[u32],
    ) {
        let live = self.arena.region(rh).map(|r| (r.owner, r.kind));
        let joins = self.layout_touched.is_some() && old.is_empty() && !new.is_empty();
        let id = (joins && live.is_some()).then(|| self.region_id(rh));
        match (id, live) {
            (Some(id), Some((owner, kind))) if !self.layout_scope.has_node(id) => {
                let row = RegionRow {
                    rh,
                    id,
                    owner,
                    is_fontstring: matches!(kind, crate::widget::RegionKind::FontString),
                };
                let owner_id = self.frame_to_id.get(&owner).copied();
                self.layout_scope.add_region(row, new, owner_id);
                self.touch_layout_node(id);
            }
            _ => self.touch_layout(),
        }
    }

    /// The ids region `rh`'s rect reads, its anchor targets and owner: its graph edges.
    fn layout_reads_of(&self, rh: RegionHandle) -> Vec<u32> {
        let mut reads: Vec<u32> = self
            .region_data
            .get(&rh)
            .map(|d| d.anchors.iter().map(|a| a.relative_to).collect())
            .unwrap_or_default();
        let owner = self.arena.region(rh).map(|r| r.owner);
        reads.extend(owner.and_then(|o| self.frame_to_id.get(&o).copied()));
        reads
    }

    /// Region `rh`, about to be freed (its data and id still readable). One that never joined the
    /// graph (unanchored, so it held no rect) moves nothing; a node nothing reads leaves the
    /// roster in place; a node another rect reads is a structural change for the full touch, as
    /// its dependents re-solve without a hash of theirs moving.
    pub(crate) fn touch_layout_freed_region(&mut self, rh: RegionHandle) {
        if self.layout_touched.is_none() {
            return self.touch_layout();
        }
        let id = self.region_to_id.get(&rh).copied();
        let reads = self.layout_reads_of(rh);
        match id {
            Some(id) if self.layout_scope.has_node(id) => {
                if self.layout_scope.remove_unread_region(id, &reads) {
                    self.bump_layout_epoch();
                } else {
                    self.touch_layout();
                }
            }
            _ => self.bump_layout_epoch(),
        }
    }

    /// `SetClampedToScreen` on frame `h`: the clamp and its extent are the frame's layout input,
    /// synced here as a derivation would, and the frame names itself.
    pub(crate) fn touch_layout_clamp(&mut self, h: FrameHandle) {
        let clamp = self.arena.is_clamped_to_screen(h);
        let (right, top) = (self.screen.right, self.screen.top);
        let input = self.layout_inputs.entry(h).or_default();
        input.clamp = clamp;
        if clamp {
            input.extent_x = right;
            input.extent_y = top;
        }
        self.touch_layout_frame(h);
    }

    /// A button slot repointed: the slot map is no layout input, the incoming region's own state
    /// is (an outgoing one is freed by the caller, its own touch).
    pub(crate) fn touch_layout_slot(&mut self, rh: Option<RegionHandle>) {
        match rh {
            Some(rh) => self.touch_layout_region_write(rh),
            None => self.bump_layout_epoch(),
        }
    }
}

#[cfg(test)]
#[path = "layout_grow_tests.rs"]
mod tests;
