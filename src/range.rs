use serde::Deserialize;
use serde::Serialize;

/// Component of a [Range].
///
/// Can be an interval or an individual value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RangeItem {
    /// Interval (inclusive).
    Interval(f64, f64),
    /// Exact value.
    Value(f64),
}

/// Range of possible values, comprised of individual values and/or intervals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    items: Vec<RangeItem>,
}

impl Range {
    /// Create a [`Range`] from [`RangeItems`](RangeItem).
    pub fn new(items: Vec<RangeItem>) -> Self {
        Self { items }
    }
    /// Check if the [`Range`] contains the `value`.
    pub fn contains(&self, value: f64) -> bool {
        for item in &self.items {
            match *item {
                RangeItem::Interval(a, b) => {
                    if a <= value && value <= b {
                        return true;
                    }
                }
                RangeItem::Value(v) => {
                    if (v - value).abs() <= f64::EPSILON {
                        return true;
                    }
                }
            }
        }
        false
    }
    /// Returns the value in [`Range`] that is closest to the given `value` or `None`, if the
    /// [`Range`] is empty.
    pub fn closest(&self, value: f64) -> Option<f64> {
        fn closer(target: f64, closest: Option<f64>, current: f64) -> f64 {
            match closest {
                Some(c) => {
                    if (target - current).abs() < (c - target).abs() {
                        current
                    } else {
                        c
                    }
                }
                None => current,
            }
        }

        if self.contains(value) {
            Some(value)
        } else {
            let mut close = None;
            for i in self.items.iter() {
                match i {
                    RangeItem::Interval(a, b) => {
                        close = Some(closer(value, close, *a));
                        close = Some(closer(value, close, *b));
                    }
                    RangeItem::Value(a) => {
                        close = Some(closer(value, close, *a));
                    }
                }
            }
            close
        }
    }
    /// Returns the smallest value in [`Range`] that is as big as the given `value` or bigger.
    /// Returns `None`, if the [`Range`] is empty or if all values are smaller than the given value.
    pub fn at_least(&self, value: f64) -> Option<f64> {
        fn closer_at_least(target: f64, closest: Option<f64>, current: f64) -> Option<f64> {
            match closest {
                Some(c) => {
                    if (target - current).abs() < (c - target).abs() && current >= target {
                        Some(current)
                    } else {
                        closest
                    }
                }
                None => {
                    if current >= target {
                        Some(current)
                    } else {
                        None
                    }
                }
            }
        }

        if self.contains(value) {
            Some(value)
        } else {
            let mut close = None;
            for i in self.items.iter() {
                match i {
                    RangeItem::Interval(a, b) => {
                        close = closer_at_least(value, close, *a);
                        close = closer_at_least(value, close, *b);
                    }
                    RangeItem::Value(a) => {
                        close = closer_at_least(value, close, *a);
                    }
                }
            }
            close
        }
    }
    /// Returns the largest value in [`Range`] that is as big as the given `value` or smaller.
    /// Returns `None`, if the [`Range`] is empty or if all values are bigger than the given `value`.
    pub fn at_max(&self, value: f64) -> Option<f64> {
        fn closer_at_max(target: f64, closest: Option<f64>, current: f64) -> Option<f64> {
            match closest {
                Some(c) => {
                    if (target - current).abs() < (c - target).abs() && current <= target {
                        Some(current)
                    } else {
                        closest
                    }
                }
                None => {
                    if current <= target {
                        Some(current)
                    } else {
                        None
                    }
                }
            }
        }

        if self.contains(value) {
            Some(value)
        } else {
            let mut close = None;
            for i in self.items.iter() {
                match i {
                    RangeItem::Interval(a, b) => {
                        close = closer_at_max(value, close, *a);
                        close = closer_at_max(value, close, *b);
                    }
                    RangeItem::Value(a) => {
                        close = closer_at_max(value, close, *a);
                    }
                }
            }
            close
        }
    }
    /// Merges two [`Ranges`](Range).
    pub fn merge(&mut self, mut r: Range) {
        self.items.append(&mut r.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_empty() {
        let r = Range::new(Vec::new());
        assert!(!r.contains(123.0));
    }
    #[test]
    fn contains() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
        ]);
        assert!(r.contains(123.0));
        assert!(r.contains(23.0));
        assert!(r.contains(42.0));
        assert!(r.contains(40.0));
        assert!(!r.contains(19.0));
    }
    #[test]
    fn closest() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
        ]);
        assert_eq!(r.closest(100.0), Some(123.0));
        assert_eq!(r.closest(1000.0), Some(123.0));
        assert_eq!(r.closest(30.0), Some(30.0));
        assert_eq!(r.closest(20.0), Some(23.0));
        assert_eq!(r.closest(50.0), Some(42.0));
    }
    #[test]
    fn at_least() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
        ]);
        assert_eq!(r.at_least(100.0), Some(123.0));
        assert_eq!(r.at_least(1000.0), None);
        assert_eq!(r.at_least(30.0), Some(30.0));
        assert_eq!(r.at_least(10.0), Some(23.0));
    }
    #[test]
    fn at_max() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
        ]);
        assert_eq!(r.at_max(100.0), Some(42.0));
        assert_eq!(r.at_max(10.0), None);
        assert_eq!(r.at_max(30.0), Some(30.0));
        assert_eq!(r.at_max(50.0), Some(42.0));
    }
}
