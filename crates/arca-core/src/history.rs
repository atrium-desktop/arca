//! Back/forward navigation history.

/// A linear stack of visited paths with a cursor. `push` truncates any
/// forward entries, mirroring browser semantics.
#[derive(Clone, Debug, Default)]
pub struct History {
    stack: Vec<String>,
    /// Index of the current entry; `usize::MAX` when empty.
    pos: usize,
}

impl History {
    pub fn new() -> History {
        History::default()
    }

    /// Visit `path`, dropping any forward history.
    pub fn push(&mut self, path: &str) {
        let keep = if self.is_empty() { 0 } else { self.pos + 1 };
        self.stack.truncate(keep);
        // Re-visiting the current path (e.g. refresh) must not duplicate it.
        if self.stack.last().map(String::as_str) == Some(path) {
            return;
        }
        self.stack.push(path.to_string());
        self.pos = self.stack.len() - 1;
    }

    pub fn current(&self) -> Option<&str> {
        if self.is_empty() {
            None
        } else {
            Some(&self.stack[self.pos])
        }
    }

    pub fn can_go_back(&self) -> bool {
        !self.is_empty() && self.pos > 0
    }

    pub fn can_go_forward(&self) -> bool {
        !self.is_empty() && self.pos + 1 < self.stack.len()
    }

    /// Step back and return the new current path, if any.
    pub fn go_back(&mut self) -> Option<&str> {
        if self.can_go_back() {
            self.pos -= 1;
            self.current()
        } else {
            None
        }
    }

    /// Step forward and return the new current path, if any.
    pub fn go_forward(&mut self) -> Option<&str> {
        if self.can_go_forward() {
            self.pos += 1;
            self.current()
        } else {
            None
        }
    }

    fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_step() {
        let mut h = History::new();
        assert!(h.current().is_none());
        h.push("/a");
        h.push("/b");
        h.push("/c");
        assert_eq!(h.current(), Some("/c"));
        assert!(h.can_go_back());
        assert!(!h.can_go_forward());

        assert_eq!(h.go_back(), Some("/b"));
        assert_eq!(h.go_back(), Some("/a"));
        assert_eq!(h.go_back(), None);
        assert!(!h.can_go_back());

        assert_eq!(h.go_forward(), Some("/b"));
        assert_eq!(h.go_forward(), Some("/c"));
        assert_eq!(h.go_forward(), None);
    }

    #[test]
    fn push_truncates_forward_history() {
        let mut h = History::new();
        h.push("/a");
        h.push("/b");
        h.go_back();
        h.push("/x");
        assert!(!h.can_go_forward());
        assert_eq!(h.current(), Some("/x"));
        assert_eq!(h.go_back(), Some("/a"));
    }

    #[test]
    fn revisiting_current_does_not_duplicate() {
        let mut h = History::new();
        h.push("/a");
        h.push("/a");
        assert_eq!(h.go_back(), None);
    }
}
