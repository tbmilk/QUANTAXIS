use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::{Close, Next, Reset, Update};
use std::f64::INFINITY;

#[derive(Debug, Clone)]
pub struct ExponentialMovingAverage {
    length: u32,
    k: f64,
    current: f64,
    is_new: bool,
    pub cached: Vec<f64>,
}

impl ExponentialMovingAverage {
    pub fn new(length: u32) -> Result<Self> {
        match length {
            0 => Err(IndicatorError::InvalidParameter),
            _ => {
                let k = 2f64 / (length as f64 + 1f64);
                Ok(Self {
                    length,
                    k,
                    current: 0f64,
                    is_new: true,
                    cached: vec![-INFINITY; length as usize],
                })
            }
        }
    }

    pub fn length(&self) -> u32 {
        self.length
    }
}

impl Next<f64> for ExponentialMovingAverage {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        if self.is_new {
            self.is_new = false;
            self.current = input;
        } else {
            self.current = self.k * input + (1.0 - self.k) * self.current;
        }
        self.cached.push(self.current);
        self.cached.remove(0);
        self.current
    }
}

impl Update<f64> for ExponentialMovingAverage {
    type Output = f64;

    fn update(&mut self, input: f64) -> Self::Output {
        if self.is_new {
            self.is_new = false;
            self.current = input;
        } else {
            self.current = self.k * input + (1.0 - self.k) * self.cached[(self.length - 2) as usize];
        }
        self.cached.remove((self.length - 1) as usize);
        self.cached.push(self.current);
        self.current
    }
}

impl<'a, T: Close> Next<&'a T> for ExponentialMovingAverage {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.next(input.close())
    }
}

impl<'a, T: Close> Update<&'a T> for ExponentialMovingAverage {
    type Output = f64;

    fn update(&mut self, input: &'a T) -> Self::Output {
        self.update(input.close())
    }
}

impl Reset for ExponentialMovingAverage {
    fn reset(&mut self) {
        self.current = 0.0;
        self.is_new = true;
    }
}

impl Default for ExponentialMovingAverage {
    fn default() -> Self {
        Self::new(9).unwrap()
    }
}

impl fmt::Display for ExponentialMovingAverage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "EMA({})", self.length)
    }
}
