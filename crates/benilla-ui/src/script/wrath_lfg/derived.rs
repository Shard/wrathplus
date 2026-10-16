//! Native L1 full-entry nodes (0x559840/0x559b10 and PLAYER_INFO writer).
//! Raw receipts remain untouched; only this query projection merges duplicates.
use super::{WrathLfgLock, WrathLfgPlayerLocks, WrathLfgPreview};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Node {
    pub entry: u32,
    pub active: bool,
    pub reason: u32,
    pub preview: WrathLfgPreview,
}

fn node(nodes: &mut Vec<Node>, entry: u32) -> &mut Node {
    if let Some(index) = nodes.iter().position(|node| node.entry == entry) {
        return &mut nodes[index];
    }
    nodes.push(Node {
        entry,
        ..Node::default()
    });
    nodes.last_mut().unwrap()
}

/// Locks precede previews in PLAYER_INFO. A zero reason is still an active lock.
pub(super) fn locks(rows: &[WrathLfgLock]) -> Vec<Node> {
    let mut nodes = Vec::new();
    for row in rows {
        let target = node(&mut nodes, row.entry);
        target.active = true;
        target.reason = row.reason;
    }
    nodes
}

pub(super) fn player(receipt: &WrathLfgPlayerLocks) -> Vec<Node> {
    let mut nodes = locks(&receipt.locks);
    for row in &receipt.previews {
        let target = node(&mut nodes, row.entry);
        let mut replacement = row.clone();
        // Scalar replacement must not discard items already appended to this node.
        replacement.items = std::mem::take(&mut target.preview.items);
        replacement.items.extend_from_slice(&row.items);
        target.preview = replacement;
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::WrathLfgRewardItem;

    #[test]
    fn full_entry_order_last_reason_and_preview_append_match_native_nodes() {
        let entry = 0x06000007;
        let first = WrathLfgPreview {
            entry,
            done: 255,
            money_base: u32::MAX,
            items: vec![WrathLfgRewardItem {
                item: 11,
                ..Default::default()
            }],
            ..Default::default()
        };
        let second = WrathLfgPreview {
            entry,
            xp_var: u32::MAX,
            items: vec![WrathLfgRewardItem {
                item: 12,
                ..Default::default()
            }],
            ..Default::default()
        };
        let receipt = WrathLfgPlayerLocks {
            locks: vec![
                WrathLfgLock { entry, reason: 123 },
                WrathLfgLock {
                    entry: 0x02000007,
                    reason: 456,
                },
                WrathLfgLock { entry, reason: 0 },
            ],
            previews: vec![
                first,
                second,
                WrathLfgPreview {
                    entry: 9,
                    ..Default::default()
                },
            ],
        };
        let original = receipt.clone();
        let nodes = player(&receipt);
        assert_eq!(receipt, original);
        assert_eq!(
            nodes.iter().map(|n| n.entry).collect::<Vec<_>>(),
            vec![entry, 0x02000007, 9]
        );
        assert!(nodes[0].active);
        assert_eq!(nodes[0].reason, 0);
        assert_eq!(nodes[0].preview.done, 0);
        assert_eq!(nodes[0].preview.money_base, 0);
        assert_eq!(nodes[0].preview.xp_var, u32::MAX);
        assert_eq!(
            nodes[0]
                .preview
                .items
                .iter()
                .map(|i| i.item)
                .collect::<Vec<_>>(),
            vec![11, 12]
        );
        assert!(!nodes[2].active);
        assert_eq!(nodes[2].reason, 0);
    }
}
