//! The dismount sound, the one mount-transition sound the client plays: the dismount handler
//! `0x607ce0` always plays a fixed kit, resolved once at startup by name-match to
//! `"SpiritWolf (DONOTRENAME)"` (`0x623110` → `0x8627bc`), at the dismounting unit. It is not
//! CreatureSoundData, and the attach path `0x607a00` plays nothing, so mounting is silent. A
//! remount (id → id′) is not a dismount.

use bevy::prelude::*;

use crate::net::FieldChanged;
use wrathplus_assets::WorldAssets;
use wrathplus_world::schedule::WorldStage;

use super::kit::{play_kit, KitRef, SoundCategory, SoundKits};
use super::{AudioListener, SoundConfig, SoundOutput};

/// The client's fixed dismount kit by SoundEntries `Name`, the constant at `0x8627bc`.
const DISMOUNT_KIT: &str = "SpiritWolf (DONOTRENAME)";

fn is_dismount(edge: &FieldChanged) -> bool {
    edge.is_unit()
        && edge.named_field(wrathplus_protocol::types::Field::UnitMountDisplayId)
        && edge.new == 0
}

/// Play the dismount kit on any streamed unit's `UNIT_FIELD_MOUNTDISPLAYID` edge to zero. The edge
/// stream skips creates, so streaming in unmounted is not a dismount.
fn dismount_sounds(
    mut edges: MessageReader<FieldChanged>,
    poses: Query<&Transform>,
    kits: Option<ResMut<SoundKits>>,
    assets: Option<Res<WorldAssets>>,
    mut out: NonSendMut<SoundOutput>,
    config: Res<SoundConfig>,
    listener: Res<AudioListener>,
) {
    let (Some(mut kits), Some(assets)) = (kits, assets) else {
        return;
    };
    let listener = listener.pos;
    for e in edges.read() {
        if !is_dismount(e) {
            continue;
        }
        let Ok(transform) = poses.get(e.entity) else {
            continue;
        };
        debug!("dismount kit on {:?} (was mount {})", e.entity, e.old);
        if let Err(err) = play_kit(
            &mut kits,
            &assets,
            &mut out,
            &config,
            listener,
            KitRef::Name(DISMOUNT_KIT),
            Some(transform.translation),
            SoundCategory::Sfx,
        ) {
            warn!("dismount kit ({DISMOUNT_KIT}): {err:#}");
        }
    }
}

/// Registration hook for [`super::SoundPlugin`].
pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, dismount_sounds.in_set(WorldStage::Present));
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrathplus_protocol::types::{Field, ObjectType};

    #[test]
    fn dismount_edges_keep_class_field_and_zero_transition_rules_in_both_layouts() {
        let mut edge = FieldChanged {
            entity: Entity::PLACEHOLDER,
            guid: 42,
            kind: ObjectType::Unit,
            field: Field::UnitMountDisplayId,
            offset: 0,
            old: 123,
            new: 0,
        };
        assert!(is_dismount(&edge));
        edge.kind = ObjectType::Player;
        assert!(is_dismount(&edge));
        edge.new = 456;
        assert!(!is_dismount(&edge), "remount is silent");
        edge.new = 0;
        edge.kind = ObjectType::Corpse;
        assert!(!is_dismount(&edge));
        edge.kind = ObjectType::Unit;
        edge.field = Field::UnitHealth;
        assert!(!is_dismount(&edge));
        edge.field = Field::UnitMountDisplayId;
        edge.offset = 1;
        assert!(!is_dismount(&edge));
    }
}
