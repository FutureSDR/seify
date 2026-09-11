use serde::Deserialize;
use serde::Serialize;

use crate::Error;

/// Component of a [Range].
///
/// A [RangeItem] can be an interval, a fixed value, or a step interval.
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum RangeItem {
    /// Min/max interval (inclusive).
    Interval(f64, f64),
    /// Exact, fixed value.
    Value(f64),
    /// Min/max/skip intervals.
    Step(f64, f64, f64),
}

/// Nonempty range of finite values, comprised of individual values and/or intervals.
///
/// Construction and deserialization validate all items. Use [`Range::items`] to inspect them.
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize)]
pub struct Range {
    items: Vec<RangeItem>,
}

impl<'de> Deserialize<'de> for Range {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct RangeData {
            items: Vec<RangeItem>,
        }
        let data = RangeData::deserialize(deserializer)?;
        Self::new(data.items).map_err(serde::de::Error::custom)
    }
}

impl Range {
    /// Create a nonempty [`Range`] from [`RangeItems`](RangeItem).
    ///
    /// Rejects nonfinite values, reversed bounds, and nonpositive or nonfinite steps.
    /// Step intervals must also have a finite span and finite number of steps.
    pub fn new(items: Vec<RangeItem>) -> Result<Self, Error> {
        if items.is_empty() {
            return Err(Error::invalid_argument("range", "range must not be empty"));
        }
        for item in &items {
            let valid = match *item {
                RangeItem::Value(value) => value.is_finite(),
                RangeItem::Interval(min, max) => min.is_finite() && max.is_finite() && min <= max,
                RangeItem::Step(min, max, step) => {
                    min.is_finite()
                        && max.is_finite()
                        && min <= max
                        && step.is_finite()
                        && step > 0.0
                        && (max - min).is_finite()
                        && ((max - min) / step).is_finite()
                }
            };
            if !valid {
                return Err(Error::invalid_argument(
                    "range",
                    format!("invalid range item: {item:?}"),
                ));
            }
        }
        Ok(Self { items })
    }

    /// Individual values and intervals that make up the range.
    pub fn items(&self) -> &[RangeItem] {
        &self.items
    }

    /// Returns the lower bound of the [`Range`].
    pub fn min(&self) -> f64 {
        self.items
            .iter()
            .map(|item| match *item {
                RangeItem::Interval(min, _) | RangeItem::Step(min, _, _) => min,
                RangeItem::Value(value) => value,
            })
            .reduce(f64::min)
            .expect("range is nonempty")
    }

    /// Returns the upper bound of the [`Range`].
    ///
    /// For step intervals, this uses the declared upper bound, which need not lie on a step.
    pub fn max(&self) -> f64 {
        self.items
            .iter()
            .map(|item| match *item {
                RangeItem::Interval(_, max) | RangeItem::Step(_, max, _) => max,
                RangeItem::Value(value) => value,
            })
            .reduce(f64::max)
            .expect("range is nonempty")
    }

    /// Check if the [`Range`] contains the `value`.
    pub fn contains(&self, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        self.items.iter().any(|item| match *item {
            RangeItem::Interval(min, max) => min <= value && value <= max,
            RangeItem::Value(allowed) => (allowed - value).abs() <= f64::EPSILON,
            RangeItem::Step(min, max, step) => {
                if value < min || value > max {
                    return false;
                }
                let allowed = min + ((value - min) / step).round() * step;
                (allowed - value).abs() <= f64::EPSILON
            }
        })
    }

    /// Returns the value in [`Range`] closest to `value`. Ties select the smaller value.
    /// Infinite inputs select the lowest or highest supported value.
    ///
    /// # Panics
    ///
    /// Panics if `value` is NaN.
    pub fn closest(&self, value: f64) -> f64 {
        assert!(!value.is_nan(), "closest requires a non-NaN value");
        match (self.at_max(value), self.at_least(value)) {
            (Some(lower), Some(upper)) => {
                // Halving before subtraction avoids overflowing with extreme finite values.
                if value / 2.0 - lower / 2.0 <= upper / 2.0 - value / 2.0 {
                    lower
                } else {
                    upper
                }
            }
            (Some(value), None) | (None, Some(value)) => value,
            (None, None) => unreachable!("range is nonempty"),
        }
    }

    /// Returns the smallest supported value greater than or equal to `value`.
    /// Returns `None` if all values are smaller, or the input is NaN.
    pub fn at_least(&self, value: f64) -> Option<f64> {
        if value.is_nan() {
            return None;
        }
        self.items
            .iter()
            .filter_map(|item| {
                let candidate = match *item {
                    RangeItem::Interval(min, max) => (value <= max).then(|| value.max(min)),
                    RangeItem::Value(allowed) => (value <= allowed).then_some(allowed),
                    RangeItem::Step(min, max, step) => {
                        if value <= min {
                            Some(min)
                        } else if value > max {
                            None
                        } else {
                            let index = ((value - min) / step).ceil();
                            let mut candidate = min + index * step;
                            if candidate < value {
                                // Correct rounding when reconstructing the grid value.
                                let next = (index + 1.0).max(index.next_up());
                                candidate = min + next * step;
                            }
                            (candidate <= max).then_some(candidate)
                        }
                    }
                };
                candidate.filter(|candidate| *candidate >= value)
            })
            .reduce(f64::min)
    }

    /// Returns the largest supported value less than or equal to `value`.
    /// Returns `None` if all values are bigger, or the input is NaN.
    pub fn at_max(&self, value: f64) -> Option<f64> {
        if value.is_nan() {
            return None;
        }
        self.items
            .iter()
            .filter_map(|item| {
                let candidate = match *item {
                    RangeItem::Interval(min, max) => (value >= min).then(|| value.min(max)),
                    RangeItem::Value(allowed) => (value >= allowed).then_some(allowed),
                    RangeItem::Step(min, max, step) => (value >= min).then(|| {
                        let limit = value.min(max);
                        let index = ((limit - min) / step).floor();
                        let mut candidate = min + index * step;
                        if candidate > limit {
                            let previous = (index - 1.0).min(index.next_down()).max(0.0);
                            candidate = min + previous * step;
                        }
                        candidate
                    }),
                };
                candidate.filter(|candidate| *candidate <= value)
            })
            .reduce(f64::max)
    }

    /// Merges two [`Ranges`](Range), preserving their validated items.
    pub fn merge(&mut self, mut r: Range) {
        self.items.append(&mut r.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_ranges() {
        assert!(Range::new(vec![]).is_err());
        for item in [
            RangeItem::Value(f64::NAN),
            RangeItem::Value(f64::INFINITY),
            RangeItem::Interval(2.0, 1.0),
            RangeItem::Interval(f64::NEG_INFINITY, 1.0),
            RangeItem::Interval(0.0, f64::NAN),
            RangeItem::Step(2.0, 1.0, 1.0),
            RangeItem::Step(0.0, 1.0, 0.0),
            RangeItem::Step(0.0, 1.0, -1.0),
            RangeItem::Step(0.0, 1.0, f64::NAN),
            RangeItem::Step(0.0, 1.0, f64::INFINITY),
            RangeItem::Step(0.0, f64::INFINITY, 1.0),
            RangeItem::Step(-f64::MAX, f64::MAX, 1.0),
            RangeItem::Step(0.0, f64::MAX, f64::MIN_POSITIVE),
        ] {
            assert!(
                Range::new(vec![RangeItem::Value(1.0), item.clone()]).is_err(),
                "{item:?}"
            );
        }
    }

    #[test]
    fn deserialization_validates_items() {
        for json in [
            r#"{"items":[]}"#,
            r#"{"items":[{"Interval":[2.0,1.0]}]}"#,
            r#"{"items":[{"Step":[0.0,10.0,0.0]}]}"#,
            r#"{"items":[{"Value":1.0},{"Step":[0.0,10.0,-1.0]}]}"#,
        ] {
            assert!(serde_json::from_str::<Range>(json).is_err());
        }

        let json = r#"{"items":[{"Value":3.0},{"Interval":[4.0,5.0]},{"Step":[6.0,10.0,2.0]}]}"#;
        let range: Range = serde_json::from_str(json).unwrap();
        assert_eq!(range.min(), 3.0);
        assert_eq!(range.max(), 10.0);
        assert_eq!(serde_json::to_string(&range).unwrap(), json);
    }

    #[test]
    fn merge_preserves_valid_items() {
        let mut range = Range::new(vec![RangeItem::Value(3.0)]).unwrap();
        range.merge(Range::new(vec![RangeItem::Interval(-2.0, 1.0)]).unwrap());
        assert_eq!(range.items().len(), 2);
        assert_eq!(range.min(), -2.0);
        assert_eq!(range.max(), 3.0);
        assert_eq!(range.closest(2.5), 3.0);
    }

    #[test]
    fn stepped_searches_stay_on_the_grid() {
        let range = Range::new(vec![RangeItem::Step(1.0, 10.0, 2.0)]).unwrap();
        let values = [1.0_f64, 3.0, 5.0, 7.0, 9.0];
        for i in -4..=24 {
            let target = f64::from(i) / 2.0;
            let lower = values.iter().copied().rev().find(|v| *v <= target);
            let upper = values.iter().copied().find(|v| *v >= target);
            let closest = values
                .iter()
                .copied()
                .min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs()))
                .unwrap();
            assert_eq!(range.at_max(target), lower);
            assert_eq!(range.at_least(target), upper);
            assert_eq!(range.closest(target), closest);
            assert!(range.contains(closest));
        }
        assert_eq!(range.closest(f64::NEG_INFINITY), 1.0);
        assert_eq!(range.closest(f64::INFINITY), 9.0);
        assert_eq!(range.at_max(f64::NEG_INFINITY), None);
        assert_eq!(range.at_least(f64::INFINITY), None);
        assert!(!range.contains(f64::NAN));
        assert_eq!(range.at_max(f64::NAN), None);
        assert_eq!(range.at_least(f64::NAN), None);
    }

    #[test]
    fn closest_handles_extreme_finite_values() {
        let range = Range::new(vec![
            RangeItem::Value(-f64::MAX),
            RangeItem::Value(f64::MAX),
        ])
        .unwrap();
        assert_eq!(range.closest(-1e300), -f64::MAX);
        assert_eq!(range.closest(1e300), f64::MAX);
    }

    #[test]
    fn stepped_searches_handle_rounding_at_bounds() {
        let range = Range::new(vec![RangeItem::Step(-10.0, -1e-16, 1.0)]).unwrap();
        assert_eq!(range.at_max(-1e-16), Some(-1.0));
        assert_eq!(range.at_least(-1e-16), None);
        assert_eq!(range.closest(-1e-16), -1.0);
        assert_eq!(range.closest(f64::INFINITY), -1.0);

        let range = Range::new(vec![RangeItem::Step(-10.0, 1.0, 1.0)]).unwrap();
        assert_eq!(range.at_least(1e-16), Some(1.0));
        assert_eq!(range.at_max(1e-16), Some(0.0));
        assert_eq!(range.closest(1e-16), 0.0);
    }

    #[test]
    #[should_panic(expected = "closest requires a non-NaN value")]
    fn closest_rejects_nan() {
        Range::new(vec![RangeItem::Value(0.0)])
            .unwrap()
            .closest(f64::NAN);
    }

    #[test]
    fn bounds() {
        for (item, min, max) in [
            (RangeItem::Value(-5.0), -5.0, -5.0),
            (RangeItem::Interval(-20.0, -10.0), -20.0, -10.0),
            (RangeItem::Step(1.0, 10.0, 2.0), 1.0, 10.0),
        ] {
            let r = Range::new(vec![item]).unwrap();
            assert_eq!(r.min(), min);
            assert_eq!(r.max(), max);
        }

        let r = Range::new(vec![
            RangeItem::Step(100.0, 110.0, 1.0),
            RangeItem::Value(123.0),
            RangeItem::Interval(-23.0, 42.0),
        ])
        .unwrap();
        assert_eq!(r.min(), -23.0);
        assert_eq!(r.max(), 123.0);
    }

    #[test]
    fn contains() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
            RangeItem::Step(100.0, 110.0, 1.0),
        ])
        .unwrap();
        assert!(r.contains(123.0));
        assert!(r.contains(23.0));
        assert!(r.contains(42.0));
        assert!(r.contains(40.0));
        assert!(r.contains(100.0));
        assert!(r.contains(107.0));
        assert!(r.contains(110.0));
        assert!(!r.contains(19.0));
    }
    #[test]
    fn closest() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
            RangeItem::Step(100.0, 110.0, 1.0),
        ])
        .unwrap();
        assert_eq!(r.closest(122.0), 123.0);
        assert_eq!(r.closest(1000.0), 123.0);
        assert_eq!(r.closest(30.0), 30.0);
        assert_eq!(r.closest(20.0), 23.0);
        assert_eq!(r.closest(50.0), 42.0);
        assert_eq!(r.closest(99.5), 100.0);
        assert_eq!(r.closest(105.3), 105.0);
        assert_eq!(r.closest(105.8), 106.0);
        assert_eq!(r.closest(109.8), 110.0);
        assert_eq!(r.closest(113.8), 110.0);
    }
    #[test]
    fn at_least() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
            RangeItem::Step(100.0, 110.0, 1.0),
        ])
        .unwrap();
        assert_eq!(r.at_least(120.0), Some(123.0));
        assert_eq!(r.at_least(1000.0), None);
        assert_eq!(r.at_least(30.0), Some(30.0));
        assert_eq!(r.at_least(10.0), Some(23.0));
        assert_eq!(r.at_least(99.0), Some(100.0));
        assert_eq!(r.at_least(105.5), Some(106.0));
    }
    #[test]
    fn at_max() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
            RangeItem::Step(100.0, 110.0, 1.0),
        ])
        .unwrap();
        assert_eq!(r.at_max(90.0), Some(42.0));
        assert_eq!(r.at_max(10.0), None);
        assert_eq!(r.at_max(30.0), Some(30.0));
        assert_eq!(r.at_max(50.0), Some(42.0));
        assert_eq!(r.at_max(101.0), Some(101.0));
        assert_eq!(r.at_max(100.3), Some(100.0));
        assert_eq!(r.at_max(111.3), Some(110.0));
    }
}
