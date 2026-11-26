//! Snapshot-based undo/redo.

use crate::document::Document;

pub struct History {
    undo: Vec<Document>,
    redo: Vec<Document>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), limit: 200 }
    }
}

impl History {
    /// Record the state *before* a change.
    pub fn push(&mut self, before: Document) {
        if self.undo.last() == Some(&before) {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &Document) -> Option<Document> {
        let prev = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(prev)
    }

    pub fn redo(&mut self, current: &Document) -> Option<Document> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_cycle() {
        let mut h = History::default();
        let a = Document::new("a", 1.0, 1.0);
        let mut b = a.clone();
        b.name = "b".into();
        h.push(a.clone());
        let back = h.undo(&b).unwrap();
        assert_eq!(back.name, "a");
        let fwd = h.redo(&back).unwrap();
        assert_eq!(fwd.name, "b");
        assert!(h.redo(&fwd).is_none());
    }
}
