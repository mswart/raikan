use std::fmt::Write;

#[derive(PartialEq, Eq, Copy, Clone)]
pub struct PositionSet {
    max: u8,
    bits: u8,
}

impl std::fmt::Debug for PositionSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for pos in 0..self.max {
            if (1 << pos) & self.bits > 0 {
                f.write_char('1')?;
            } else {
                f.write_char('0')?;
            }
        }
        Ok(())
    }
}

/// This class implements a set data structure over a small interger range. It is designed
/// specifically for the ranges 0..5 and 0..6 but 0..8 is also supported.
/// The ranges always start with 0. The maximal possible number is stored, too. It is simply called
/// `max`. If you want to get the largest set value (use `last`).
/// The provided interface and functions is orientied on other set implementations like
/// BTreeSet.
/// Furthermore, the class implements many bit-wise operations that work element-wise.
impl PositionSet {
    pub fn new(position_count: u8) -> Self {
        assert!(
            position_count <= 8,
            "At most eight positions are supported ({} requested)",
            position_count
        );
        Self {
            max: position_count,
            bits: 0,
        }
    }

    pub fn create(position_count: u8, initial: u8) -> Self {
        assert!(
            position_count <= 8,
            "At most eight positions are supported ({} requested)",
            position_count
        );
        Self {
            max: position_count,
            bits: initial & ((1 << position_count) - 1),
        }
    }

    pub fn add(&mut self, position: u8) {
        assert!(
            position < self.max,
            "Position {} out-of-bounds (0..{})",
            position,
            self.max,
        );
        self.bits |= 1 << position
    }

    /// Ensures a given position field is unset.
    /// Idempotent operation: previous value is not checked.
    ///
    /// ```
    /// use hanabi::PositionSet;
    /// let mut a = PositionSet::new(5);
    /// a.add(3);
    /// a.remove(3);
    /// assert_eq!(a, PositionSet::new(5));
    /// a.remove(3);
    /// assert_eq!(a, PositionSet::new(5));
    /// a.add(1);
    /// a.add(3);
    /// a.remove(3);
    /// let mut b = PositionSet::new(5);
    /// b.add(1);
    /// assert_eq!(a, b);
    /// ```
    pub fn remove(&mut self, position: u8) {
        assert!(
            position < self.max,
            "Position {} out-of-bounds (0..{})",
            position,
            self.max,
        );
        self.bits &= !(1 << position);
    }

    pub fn contains(&self, position: u8) -> bool {
        assert!(
            position < self.max,
            "Position {} out-of-bounds (0..{})",
            position,
            self.max,
        );
        self.bits & (1 << position) > 0
    }

    /// Returns the lowest set position or None
    ///
    /// ```
    /// use hanabi::PositionSet;
    /// let mut a = PositionSet::new(5);
    /// assert!(a.first().is_none());
    /// a.add(3);
    /// assert_eq!(a.first().expect("position set"), 3);
    /// a.add(2);
    /// assert_eq!(a.first().expect("position set"), 2);
    /// ```
    pub fn first(&self) -> Option<u8> {
        if self.bits == 0 {
            return None;
        }
        Some(self.bits.trailing_zeros() as u8)
    }

    /// Returns the highest set position or None
    ///
    /// ```
    /// use hanabi::PositionSet;
    /// let mut a = PositionSet::new(5);
    /// assert!(a.last().is_none());
    /// a.add(2);
    /// assert_eq!(a.last().expect("position set"), 2);
    /// a.add(3);
    /// assert_eq!(a.last().expect("position set"), 3);
    /// ```
    pub fn last(&self) -> Option<u8> {
        if self.bits == 0 {
            return None;
        }
        Some(7 - self.bits.leading_zeros() as u8)
    }

    pub fn max(&self) -> u8 {
        self.max
    }

    /// Returns the number of positions that are currently set
    ///
    /// ```
    /// use hanabi::PositionSet;
    /// let mut a = PositionSet::new(5);
    /// assert_eq!(a.len(), 0);
    /// a.add(3);
    /// assert_eq!(a.len(), 1);
    /// ```
    pub fn len(&self) -> u8 {
        self.bits.count_ones() as u8
    }

    /// Is no position set at all (equals to len() == 0)
    ///
    /// ```
    /// use hanabi::PositionSet;
    /// let mut a = PositionSet::new(5);
    /// assert!(a.is_empty());
    /// a.add(3);
    /// assert!(!a.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Are all positions set (equals to len() == max())
    ///
    /// ```
    /// use hanabi::PositionSet;
    /// let mut a = PositionSet::new(3);
    /// assert!(!a.is_full());
    /// a.add(0);
    /// a.add(1);
    /// assert!(!a.is_full());
    /// a.add(2);
    /// assert!(a.is_full());
    /// ```
    pub fn is_full(&self) -> bool {
        self.len() == self.max
    }

    pub fn iter(&self) -> PositionSetIterator {
        PositionSetIterator {
            offset: 0,
            remaining: self.bits,
            first: None,
        }
    }

    pub fn iter_first(&self, first: u8) -> PositionSetIterator {
        PositionSetIterator {
            offset: 0,
            remaining: self.bits,
            first: Some(first),
        }
    }
}

pub struct PositionSetIterator {
    offset: u8,
    remaining: u8,
    first: Option<u8>,
}

impl Iterator for PositionSetIterator {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if let Some(first) = self.first {
            self.remaining &= !(1 << first);
            self.first = None;
            return Some(first);
        }
        if self.remaining == 0 {
            return None;
        }
        let step = self.remaining.trailing_zeros() + 1;

        self.remaining >>= step;
        self.offset += step as u8;

        Some(self.offset - 1)
    }
}

impl std::ops::BitOr for PositionSet {
    type Output = PositionSet;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::Output {
            max: self.max,
            bits: self.bits | rhs.bits,
        }
    }
}

impl std::ops::BitAnd for PositionSet {
    type Output = PositionSet;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self::Output {
            max: self.max,
            bits: self.bits & rhs.bits,
        }
    }
}

impl std::ops::Not for PositionSet {
    type Output = PositionSet;

    fn not(self) -> Self::Output {
        Self::Output {
            max: self.max,
            bits: !self.bits & ((1 << self.max) as u16 - 1) as u8,
        }
    }
}

impl std::ops::Sub for PositionSet {
    type Output = PositionSet;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            max: self.max,
            bits: self.bits & !rhs.bits,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_empty() {
        assert_eq!(PositionSet::new(0), PositionSet::new(0));
        assert_ne!(PositionSet::new(0), PositionSet::new(1));
        assert_eq!(PositionSet::new(1), PositionSet::new(1));
    }

    #[test]
    #[should_panic(expected = "At most eight positions are supported")]
    fn fail_too_large() {
        PositionSet::new(9);
    }

    #[test]
    fn add_and_contains() {
        let mut set = PositionSet::new(5);
        assert!(!set.contains(1));
        set.add(1);
        assert!(set.contains(1));
    }

    #[test]
    #[should_panic(expected = "Position 7 out-of-bounds (0..5)")]
    fn fail_out_of_bound_set() {
        let mut set = PositionSet::new(5);
        set.add(7);
    }

    #[test]
    #[should_panic(expected = "Position 7 out-of-bounds (0..5)")]
    fn fail_out_of_bound_test() {
        let set = PositionSet::new(5);
        set.contains(7);
    }

    #[test]
    fn test_or() {
        let mut set1 = PositionSet::new(5);
        set1.add(1);
        set1.add(2);
        let mut set2 = PositionSet::new(5);
        set2.add(2);
        set2.add(4);
        let joined = set1 | set2;
        assert!(!joined.contains(0));
        assert!(joined.contains(1));
        assert!(joined.contains(2));
        assert!(joined.contains(4));
    }

    #[test]
    fn test_and() {
        let mut set1 = PositionSet::new(5);
        set1.add(1);
        set1.add(2);
        let mut set2 = PositionSet::new(5);
        set2.add(2);
        set2.add(4);
        let joined = set1 & set2;
        assert!(!joined.contains(0));
        assert!(!joined.contains(1));
        assert!(joined.contains(2));
        assert!(!joined.contains(4));
    }

    #[test]
    fn test_not() {
        let mut set = PositionSet::new(5);
        set.add(0);
        set.add(3);
        let inverted = !set;
        assert_eq!(inverted.len(), set.max - set.len());
        assert!(!inverted.contains(0));
        assert!(inverted.contains(1));
        assert!(inverted.contains(2));
        assert!(!inverted.contains(3));
        assert!(inverted.contains(4));
    }

    #[test]
    fn test_not_full() {
        let mut set = PositionSet::new(8);
        set.add(0);
        set.add(3);
        let inverted = !set;
        assert_eq!(inverted.len(), set.max - set.len());
        assert!(!inverted.contains(0));
        assert!(inverted.contains(1));
        assert!(inverted.contains(2));
        assert!(!inverted.contains(3));
        assert!(inverted.contains(4));
    }

    #[test]
    fn test_iterator() {
        let mut set = PositionSet::new(8);
        assert_eq!(set.iter().count(), 0);
        set.add(3);
        set.add(1);
        assert_eq!(set.iter().count(), 2);
        let values: Vec<u8> = set.iter().collect();
        assert_eq!(values, vec![1, 3]);
    }
}
