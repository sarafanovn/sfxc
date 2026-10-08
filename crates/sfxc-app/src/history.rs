//! In-memory undo/redo for the open sound. Independent of saved versions.

use sfxc_core::patch::SoundPatch;

pub struct History {
    undo: Vec<SoundPatch>,
    redo: Vec<SoundPatch>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), limit }
    }

    pub fn push(&mut self, before: SoundPatch) {
        if self.undo.last() == Some(&before) {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &SoundPatch) -> Option<SoundPatch> {
        let prev = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(prev)
    }

    pub fn redo(&mut self, current: &SoundPatch) -> Option<SoundPatch> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(seed: u64) -> SoundPatch {
        SoundPatch { seed, ..Default::default() }
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut h = History::new(10);
        h.push(p(1));
        h.push(p(2));
        assert_eq!(h.undo(&p(3)), Some(p(2)));
        assert_eq!(h.undo(&p(2)), Some(p(1)));
        assert_eq!(h.undo(&p(1)), None);
        assert_eq!(h.redo(&p(1)), Some(p(2)));
        assert_eq!(h.redo(&p(2)), Some(p(3)));
        assert_eq!(h.redo(&p(3)), None);
    }

    #[test]
    fn push_clears_redo_and_respects_limit() {
        let mut h = History::new(2);
        h.push(p(1));
        h.push(p(2));
        h.push(p(3));
        assert_eq!(h.undo(&p(4)), Some(p(3)));
        h.push(p(5));
        assert_eq!(h.redo(&p(6)), None);
        assert_eq!(h.undo(&p(6)), Some(p(5)));
        assert_eq!(h.undo(&p(5)), Some(p(2)));
        assert_eq!(h.undo(&p(2)), None);
    }

    #[test]
    fn duplicate_push_ignored() {
        let mut h = History::new(10);
        h.push(p(1));
        h.push(p(1));
        assert_eq!(h.undo(&p(2)), Some(p(1)));
        assert_eq!(h.undo(&p(1)), None);
    }
}
