use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::{Close, Next, Reset};

#[derive(Debug, Clone)]
pub struct StandardDeviation {
    n: u32,
    index: usize,
    count: u32,
    m: f64,
    m2: f64,
    vec: Vec<f64>,
}

impl StandardDeviation {
    pub fn new(n: u32) -> Result<Self> {
        match n {
            0 => Err(IndicatorError::InvalidParameter),
            _ => Ok(StandardDeviation {
                n,
                index: 0,
                count: 0,
                m: 0.0,
                m2: 0.0,
                vec: vec![0.0; n as usize],
            }),
        }
    }

    pub(super) fn mean(&self) -> f64 {
        self.m
    }
}

impl Next<f64> for StandardDeviation {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        self.index = (self.index + 1) % (self.n as usize);
        let old_val = self.vec[self.index];
        self.vec[self.index] = input;

        if self.count < self.n {
            self.count += 1;
            let delta = input - self.m;
            self.m += delta / self.count as f64;
            let delta2 = input - self.m;
            self.m2 += delta * delta2;
        } else {
            let delta = input - old_val;
            let old_m = self.m;
            self.m += delta / self.n as f64;
            let delta2 = input - self.m + old_val - old_m;
            self.m2 += delta * delta2;
        }

        (self.m2 / self.count as f64).sqrt()
    }
}

impl<'a, T: Close> Next<&'a T> for StandardDeviation {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.next(input.close())
    }
}

impl Reset for StandardDeviation {
    fn reset(&mut self) {
        self.index = 0;
        self.count = 0;
        self.m = 0.0;
        self.m2 = 0.0;
        for i in 0..(self.n as usize) {
            self.vec[i] = 0.0;
        }
    }
}

impl Default for StandardDeviation {
    fn default() -> Self {
        Self::new(9).unwrap()
    }
}

impl fmt::Display for StandardDeviation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SD({})", self.n)
    }
}
