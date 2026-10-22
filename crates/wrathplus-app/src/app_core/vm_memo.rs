//! Host-side memory about the UI VM, which lives for one login: the reference builds it at world
//! entry and destroys it at the character screen (`0x48fbf0`, `0x490bd0`). A seed or change memo
//! kept past its VM silently skips the push into the next one; [`VmMemo`] keys it on
//! [`UiScript::session`], so against a new VM it reads as fresh.

use wrathplus_ui::script::UiScript;

/// A host-side memo valid only for the VM that wrote it: [`VmMemo::get`] resets it to
/// `T::default()` when read against another.
pub(crate) struct VmMemo<T> {
    /// The VM this memory is about; 0 is no VM, as [`UiScript::session`] counts from 1.
    session: u64,
    inner: T,
}

impl<T: Default> Default for VmMemo<T> {
    fn default() -> Self {
        Self {
            session: 0,
            inner: T::default(),
        }
    }
}

impl<T: Default> VmMemo<T> {
    /// The memory, cleared first against a different VM.
    pub(crate) fn get(&mut self, script: &UiScript) -> &mut T {
        self.get_for(Some(script))
    }

    /// [`VmMemo::get`] for a system that also runs with no VM, at the character screen; no VM is
    /// session 0, so the memo holds there too.
    pub(crate) fn get_for(&mut self, script: Option<&UiScript>) -> &mut T {
        self.get_reset_for(script).0
    }

    /// [`VmMemo::get`], plus whether this read reset it: a gated feed must re-push into a fresh VM
    /// even when every input is unchanged.
    pub(crate) fn get_reset(&mut self, script: &UiScript) -> (&mut T, bool) {
        self.get_reset_for(Some(script))
    }

    fn get_reset_for(&mut self, script: Option<&UiScript>) -> (&mut T, bool) {
        let now = script.map_or(0, UiScript::session);
        let reset = self.session != now;
        if reset {
            self.session = now;
            self.inner = T::default();
        }
        (&mut self.inner, reset)
    }
}

impl VmMemo<bool> {
    /// True exactly once per VM, for a seed pushed once per login.
    pub(crate) fn claim(&mut self, script: &UiScript) -> bool {
        let done = self.get(script);
        if *done {
            false
        } else {
            *done = true;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memo_does_not_survive_the_vm_it_was_written_against() {
        let first = UiScript::new().expect("VM");
        let second = UiScript::new().expect("VM");
        assert_ne!(
            first.session(),
            second.session(),
            "every VM gets its own identity"
        );

        let mut memo: VmMemo<Vec<u32>> = VmMemo::default();
        memo.get(&first).push(7);
        assert_eq!(memo.get(&first), &vec![7], "…and keeps it within one VM");
        assert!(
            memo.get(&second).is_empty(),
            "a memo about the previous VM reads as empty against the next one"
        );
        assert!(
            memo.get(&first).is_empty(),
            "and moving back does not resurrect it — the memory is gone, not shelved"
        );
    }

    #[test]
    fn get_reset_reports_each_session_flip_once() {
        let first = UiScript::new().expect("VM");
        let second = UiScript::new().expect("VM");
        let mut memo: VmMemo<u32> = VmMemo::default();

        let (m, reset) = memo.get_reset(&first);
        assert!(reset, "the first read of a session is the reset");
        *m = 7;
        let (m, reset) = memo.get_reset(&first);
        assert!(!reset, "steady frames read quietly");
        assert_eq!(*m, 7);
        let (m, reset) = memo.get_reset(&second);
        assert!(reset, "a new VM resets again");
        assert_eq!(*m, 0, "…and the memory is gone with the old one");
    }

    #[test]
    fn a_seed_is_claimed_once_per_vm() {
        let first = UiScript::new().expect("VM");
        let second = UiScript::new().expect("VM");
        let mut seeded = VmMemo::<bool>::default();

        assert!(seeded.claim(&first), "the first VM needs seeding");
        assert!(!seeded.claim(&first), "…and only once");
        assert!(seeded.claim(&second), "the next VM needs it again");
        assert!(!seeded.claim(&second));
    }
}
