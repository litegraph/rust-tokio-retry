use std::time::Duration;
use std::iter::Iterator;

/// A retry strategy driven by a fixed interval.
#[derive(Clone)]
pub struct FixedInterval {
    duration: Duration
}

impl FixedInterval {
    /// Constructs a new fixed interval strategy.
    pub fn new(duration: Duration) -> FixedInterval {
        FixedInterval{duration: duration}
    }
}

impl Iterator for FixedInterval {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        Some(self.duration)
    }
}

#[test]
fn returns_some_fixed() {
    let mut s = FixedInterval::new(Duration::from_millis(123));

    assert_eq!(s.next(), Some(Duration::from_millis(123)));
    assert_eq!(s.next(), Some(Duration::from_millis(123)));
    assert_eq!(s.next(), Some(Duration::from_millis(123)));
}
