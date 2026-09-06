#[derive(Debug)]
pub struct Budget<'a> {
    depth: u32,
    size: &'a mut u32,
    reserve: u32,
}

impl<'a> Budget<'a> {
    pub fn new(depth: u32, size: &'a mut u32) -> Self {
        Budget {
            depth,
            size,
            reserve: 0,
        }
    }

    pub fn descend(&mut self) -> Budget<'_> {
        Budget {
            depth: self.depth.saturating_sub(1),
            size: &mut *self.size,
            reserve: self.reserve,
        }
    }

    pub fn with_depth(&mut self, depth: u32) -> Budget<'_> {
        Budget {
            depth,
            size: &mut *self.size,
            reserve: self.reserve,
        }
    }

    pub fn with_reserve(&mut self, depth: u32, reserve: u32) -> Budget<'_> {
        Budget {
            depth,
            size: &mut *self.size,
            reserve: self.reserve + reserve,
        }
    }

    pub fn spend(&mut self) -> bool {
        if self.size_left() == 0 {
            false
        } else {
            *self.size -= 1;
            true
        }
    }

    // spend n nodes at once for a sub-tree emitted whole rather than drawn node by node
    pub fn spend_n(&mut self, n: u32) {
        *self.size = self.size.saturating_sub(n);
    }

    pub fn exhausted(&self) -> bool {
        self.depth == 0 || self.size_left() == 0
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn size_left(&self) -> u32 {
        self.size.saturating_sub(self.reserve)
    }

    pub fn decay(&self, max_depth: u32) -> f32 {
        self.depth as f32 / max_depth.max(1) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_is_independent_across_siblings() {
        let mut size = 100;
        let mut root = Budget::new(3, &mut size);
        let mut level1 = root.descend();
        assert_eq!(level1.depth(), 2);
        let left = level1.descend();
        assert_eq!(left.depth(), 1);
        drop(left);
        let right = level1.descend();
        assert_eq!(right.depth(), 1);
    }

    #[test]
    fn size_is_shared_across_siblings() {
        let mut size = 3;
        let mut root = Budget::new(5, &mut size);
        assert!(root.spend());
        {
            let mut child = root.descend();
            assert!(child.spend());
            assert!(child.spend());
            assert!(!child.spend());
        }
        assert_eq!(root.size_left(), 0);
        assert!(root.exhausted());
    }

    #[test]
    fn exhausted_at_depth_zero() {
        let mut size = 100;
        let mut b = Budget::new(1, &mut size);
        assert!(!b.exhausted());
        let child = b.descend();
        assert!(child.exhausted());
    }

    #[test]
    fn reserve_holds_nodes_back() {
        let mut size = 5;
        let mut root = Budget::new(3, &mut size);
        let mut held = root.with_reserve(3, 2);
        assert_eq!(held.size_left(), 3);
        for _ in 0..3 {
            assert!(held.spend());
        }
        assert!(!held.spend());
        drop(held);
        assert_eq!(root.size_left(), 2);
    }
}
