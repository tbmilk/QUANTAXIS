use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::{Close, Next, Reset};

#[derive(Debug, Clone)]
pub struct SimpleMovingAverage {
    n: u32,
    index: usize,
    count: u32,
    sum: f64,
    vec: Vec<f64>,
}

impl SimpleMovingAverage {
    pub fn new(n: u32) -> Result<Self> {
        match n {
            0 => Err(IndicatorError::InvalidParameter),
            _ => Ok(Self {
                n,
                index: 0,
                count: 0,
                sum: 0.0,
                vec: vec![0.0; n as usize],
            }),
        }
    }
}

impl Next<f64> for SimpleMovingAverage {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        self.index = (self.index + 1) % (self.n as usize);
        let old_val = self.vec[self.index];
        self.vec[self.index] = input;
        if self.count < self.n {
            self.count += 1;
        }
        self.sum = self.sum - old_val + input;
        self.sum / (self.count as f64)
    }
}

impl<'a, T: Close> Next<&'a T> for SimpleMovingAverage {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.next(input.close())
    }
}

impl Reset for SimpleMovingAverage {
    fn reset(&mut self) {
        self.index = 0;
        self.count = 0;
        self.sum = 0.0;
        for i in 0..(self.n as usize) {
            self.vec[i] = 0.0;
        }
    }
}

impl Default for SimpleMovingAverage {
    fn default() -> Self {
        Self::new(9).unwrap()
    }
}

impl fmt::Display for SimpleMovingAverage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SMA({})", self.n)
    }
}
