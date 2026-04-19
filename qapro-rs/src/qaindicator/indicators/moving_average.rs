use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::{Close, Next, Reset, Update};

#[derive(Debug, Clone)]
pub struct MovingAverage {
    pub(crate) n: u32,
    index: usize,
    count: u32,
    sum: f64,
    vec: Vec<f64>,
    pub cached: Vec<f64>,
}

impl MovingAverage {
    pub fn new(n: u32) -> Result<Self> {
        match n {
            0 => Err(IndicatorError::InvalidParameter),
            _ => Ok(Self {
                n,
                index: 0,
                count: 1,
                sum: 0.0,
                vec: vec![0.0; n as usize],
                cached: vec![0.0; n as usize],
            }),
        }
    }

    pub fn is_real(&self) -> bool {
        self.count - 1 >= self.n
    }
}

impl Next<f64> for MovingAverage {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        self.index = (self.index + 1) % (self.n as usize);
        let old_val = self.vec[self.index];
        self.vec[self.index] = input;
        let mut res = 0.0;
        if self.count < self.n {
            self.sum = self.sum - old_val + input;
        } else {
            self.sum = self.sum - old_val + input;
            res = self.sum / (self.n as f64);
        }
        self.count += 1;
        self.cached.push(res);
        self.cached.remove(0);
        res
    }
}

impl<'a, T: Close> Next<&'a T> for MovingAverage {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.next(input.close())
    }
}

impl Update<f64> for MovingAverage {
    type Output = f64;

    fn update(&mut self, input: f64) -> Self::Output {
        let old_val = self.vec[self.index];
        self.vec[self.index] = input;
        let mut res = 0.0;
        self.count -= 1;
        if self.count < self.n {
            self.sum = self.sum - old_val + input;
        } else {
            self.sum = self.sum - old_val + input;
            res = self.sum / (self.n as f64);
        }
        self.count += 1;
        self.cached.remove((self.n - 1) as usize);
        self.cached.push(res);
        res
    }
}

impl<'a, T: Close> Update<&'a T> for MovingAverage {
    type Output = f64;

    fn update(&mut self, input: &'a T) -> Self::Output {
        self.update(input.close())
    }
}

impl Reset for MovingAverage {
    fn reset(&mut self) {
        self.index = 0;
        self.count = 0;
        self.sum = 0.0;
        for i in 0..(self.n as usize) {
            self.vec[i] = 0.0;
            self.cached[i] = 0.0;
        }
    }
}

impl Default for MovingAverage {
    fn default() -> Self {
        Self::new(9).unwrap()
    }
}

impl fmt::Display for MovingAverage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ma({})", self.n)
    }
}
