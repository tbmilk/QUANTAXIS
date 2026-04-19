use std::collections::VecDeque;
use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::{Close, Next, Reset};

pub struct EfficiencyRatio {
    length: u32,
    prices: VecDeque<f64>,
}

impl EfficiencyRatio {
    pub fn new(length: u32) -> Result<Self> {
        if length == 0 {
            Err(IndicatorError::InvalidParameter)
        } else {
            Ok(Self {
                length,
                prices: VecDeque::with_capacity(length as usize + 1),
            })
        }
    }
}

impl Next<f64> for EfficiencyRatio {
    type Output = f64;

    fn next(&mut self, input: f64) -> f64 {
        self.prices.push_back(input);
        if self.prices.len() <= 2 {
            return 1.0;
        }
        let first = self.prices[0];
        let volatility = self
            .prices
            .iter()
            .skip(1)
            .fold((first, 0.0), |(prev, sum), &val| {
                (val, sum + (prev - val).abs())
            })
            .1;
        let last_index = self.prices.len() - 1;
        let direction = (first - self.prices[last_index]).abs();
        if self.prices.len() > (self.length as usize) {
            self.prices.pop_front();
        }
        direction / volatility
    }
}

impl<'a, T: Close> Next<&'a T> for EfficiencyRatio {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> f64 {
        self.next(input.close())
    }
}

impl Reset for EfficiencyRatio {
    fn reset(&mut self) {
        self.prices.clear();
    }
}

impl Default for EfficiencyRatio {
    fn default() -> Self {
        Self::new(14).unwrap()
    }
}

impl fmt::Display for EfficiencyRatio {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ER({})", self.length)
    }
}
