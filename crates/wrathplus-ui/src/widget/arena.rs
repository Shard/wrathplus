//! The generational arena under [`super::WidgetArena`]: a removed slot bumps its generation, so
//! an old handle stops resolving.

struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// `remove` bumps the slot's generation, so an old handle stops resolving.
pub(super) struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

impl<T> Arena<T> {
    pub(super) fn new() -> Arena<T> {
        Arena {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    pub(super) fn insert(&mut self, value: T) -> (u32, u32) {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            (index, slot.generation)
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(Slot {
                generation: 0,
                value: Some(value),
            });
            (index, 0)
        }
    }

    pub(super) fn remove(&mut self, index: u32, generation: u32) -> Option<T> {
        let slot = self.slots.get_mut(index as usize)?;
        if slot.generation != generation {
            return None;
        }
        let taken = slot.value.take();
        if taken.is_some() {
            slot.generation = slot.generation.wrapping_add(1);
            self.free.push(index);
        }
        taken
    }

    pub(super) fn get(&self, index: u32, generation: u32) -> Option<&T> {
        let slot = self.slots.get(index as usize)?;
        if slot.generation != generation {
            return None;
        }
        slot.value.as_ref()
    }

    pub(super) fn get_mut(&mut self, index: u32, generation: u32) -> Option<&mut T> {
        let slot = self.slots.get_mut(index as usize)?;
        if slot.generation != generation {
            return None;
        }
        slot.value.as_mut()
    }

    /// Slots ever allocated, live or free: a bound on any walk over handles.
    pub(super) fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (u32, u32, &T)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.value.as_ref().map(|v| (i as u32, s.generation, v)))
    }

    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = (u32, u32, &mut T)> + '_ {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(i, s)| s.value.as_mut().map(|v| (i as u32, s.generation, v)))
    }
}
