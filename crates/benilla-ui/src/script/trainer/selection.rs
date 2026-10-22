//! Build 12340's selection seat on a fresh list: the rebuild `0x594ba0` selects the first visible
//! available service (`state == 0`, visible), else the second row's record when the list has two
//! rows, else nothing (`0xc0e49c` = 0). Labelled: the native's second row is its sorted array's;
//! here it is the row tree's.

use super::{rows, Row, TrainerServiceCategory};
use crate::script::Model;

pub(super) fn seat_first(model: &mut Model) {
    let Some(t) = model.trainer.as_ref() else {
        return;
    };
    let (all, visible) = rows(model);
    let first = all[..visible].iter().find_map(|r| match r {
        Row::Service(si) => t
            .services
            .get(*si)
            .filter(|s| s.category == TrainerServiceCategory::Available)
            .map(|s| s.spell_id),
        Row::Header(_) => None,
    });
    let second = match all.get(1) {
        Some(Row::Service(si)) => t.services.get(*si).map(|s| s.spell_id),
        _ => None,
    };
    model.trainer_selection = first.or(second);
}

#[cfg(test)]
mod tests {
    use super::super::tests::trainer;
    use crate::script::UiScript;

    #[test]
    fn selection_and_close_intents() {
        let mut s = UiScript::new().unwrap();
        s.set_trainer(Some(trainer()));
        let sel = |s: &UiScript| {
            s.eval::<Option<i64>>("return GetTrainerSelectionIndex()")
                .unwrap()
        };
        assert_eq!(
            sel(&s),
            Some(2),
            "a fresh list seats its first available service"
        );
        s.run("SelectTrainerService(5)").unwrap(); // Bloodrage
        assert_eq!(sel(&s), Some(5));
        s.run("SelectTrainerService(9)").unwrap(); // past the 5 rows: clears
        assert_eq!(sel(&s), None, "nothing selected answers nil (`0x594430`)");
        s.set_trainer(Some(trainer()));
        assert_eq!(
            sel(&s),
            None,
            "a snapshot does not re-seat a cleared selection"
        );

        assert!(!s.take_trainer_close());
        s.run("CloseTrainer()").unwrap();
        assert!(s.take_trainer_close());
        assert!(!s.take_trainer_close(), "drained");
    }

    /// A fresh list re-seats the selection (12340 `0x594ba0`), so a re-opened trainer never keeps the
    /// last visit's.
    #[test]
    fn a_new_list_packet_clears_the_selection() {
        let mut s = UiScript::new().unwrap();
        s.set_trainer(Some(trainer()));
        s.run("SelectTrainerService(5)").unwrap();
        assert_eq!(
            s.eval::<i64>("return GetTrainerSelectionIndex()").unwrap(),
            5
        );
        s.reset_trainer_list_state();
        s.set_trainer(Some(trainer()));
        assert_eq!(
            s.eval::<i64>("return GetTrainerSelectionIndex()").unwrap(),
            2,
            "the rebuild re-seats the first available service, not the last visit's"
        );
    }
}
