use std::collections::VecDeque;
use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::{Close, Next, Reset};

#[derive(Debug, Clone)]
pub struct RateOfChange {
    length: u32,
    prices: VecDeque<f64>,
}

impl RateOfChange {
    pub fn new(length: u32) -> Result<Self> {
        match length {
            0 => Err(IndicatorError::InvalidParameter),
            _ => Ok(Self {
                length,
                prices: VecDeque::with_capacity(length as usize + 1),
            }),
        }
    }
}

impl Next<f64> for RateOfChange {
    type Output = f64;

    fn next(&mut self, input: f64) -> f64 {
        self.prices.push_back(input);
        if self.prices.len() == 1 {
            return 0.0;
        }
        let initial_price = if self.prices.len() > (self.length as usize) {
            self.prices.pop_front().unwrap()
        } else {
            self.prices[0]
        };
        (input - initial_price) / initial_price * 100.0
    }
}

impl<'a, T: Close> Next<&'a T> for RateOfChange {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> f64 {
        self.next(input.close())
    }
}

impl Default for RateOfChange {
    fn default() -> Self {
        Self::new(9).unwrap()
    }
}

impl fmt::Display for RateOfChange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ROC({})", self.length)
    }
}

impl Reset for RateOfChange {
    fn reset(&mut self) {
        self.prices.clear();
    }
}
