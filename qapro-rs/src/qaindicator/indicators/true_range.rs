use std::fmt;
use crate::qaindicator::{Close, High, Low, Next, Reset, Update};
use crate::qaindicator::max3;

#[derive(Debug, Clone)]
pub struct TrueRange {
    prev_closeque: Vec<f64>,
}

impl TrueRange {
    pub fn new() -> Self {
        Self { prev_closeque: vec![] }
    }
}

impl Default for TrueRange {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TrueRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TRUE_RANGE()")
    }
}

impl Next<f64> for TrueRange {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        if self.prev_closeque.is_empty() {
            self.prev_closeque.push(input);
            0.0
        } else {
            let prev = self.prev_closeque[self.prev_closeque.len() - 1];
            let distance = (input - prev).abs();
            self.prev_closeque.push(input);
            distance
        }
    }
}

impl Update<f64> for TrueRange {
    type Output = f64;

    fn update(&mut self, input: f64) -> Self::Output {
        if self.prev_closeque.len() < 2 {
            let u = self.prev_closeque.last_mut().unwrap();
            *u = input;
            0.0
        } else {
            let prev = self.prev_closeque[self.prev_closeque.len() - 2];
            let distance = (input - prev).abs();
            let u = self.prev_closeque.last_mut().unwrap();
            *u = input;
            distance
        }
    }
}

impl<'a, T: High + Low + Close> Next<&'a T> for TrueRange {
    type Output = f64;

    fn next(&mut self, bar: &'a T) -> Self::Output {
        if self.prev_closeque.is_empty() {
            self.prev_closeque.push(bar.close());
            bar.high() - bar.low()
        } else {
            let prev_close = self.prev_closeque[self.prev_closeque.len() - 1];
            let dist1 = bar.high() - bar.low();
            let dist2 = (bar.high() - prev_close).abs();
            let dist3 = (bar.low() - prev_close).abs();
            let max_dist = max3(dist1, dist2, dist3);
            self.prev_closeque.push(bar.close());
            max_dist
        }
    }
}

impl<'a, T: High + Low + Close> Update<&'a T> for TrueRange {
    type Output = f64;

    fn update(&mut self, bar: &'a T) -> Self::Output {
        if self.prev_closeque.len() < 2 {
            let u = self.prev_closeque.last_mut().unwrap();
            *u = bar.close();
            bar.high() - bar.low()
        } else {
            let prev_close = self.prev_closeque[self.prev_closeque.len() - 2];
            let dist1 = bar.high() - bar.low();
            let dist2 = (bar.high() - prev_close).abs();
            let dist3 = (bar.low() - prev_close).abs();
            let max_dist = max3(dist1, dist2, dist3);
            let u = self.prev_closeque.last_mut().unwrap();
            *u = bar.close();
            max_dist
        }
    }
}

impl Reset for TrueRange {
    fn reset(&mut self) {
        self.prev_closeque = vec![];
    }
}
