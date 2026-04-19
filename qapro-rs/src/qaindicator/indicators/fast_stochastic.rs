use std::fmt;
use crate::qaindicator::errors::Result;
use crate::qaindicator::indicators::{Maximum, Minimum};
use crate::qaindicator::{Close, High, Low, Next, Reset};

#[derive(Debug, Clone)]
pub struct FastStochastic {
    length: u32,
    minimum: Minimum,
    maximum: Maximum,
}

impl FastStochastic {
    pub fn new(length: u32) -> Result<Self> {
        Ok(Self {
            length,
            minimum: Minimum::new(length)?,
            maximum: Maximum::new(length)?,
        })
    }

    pub fn length(&self) -> u32 {
        self.length
    }
}

impl Next<f64> for FastStochastic {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        let min = self.minimum.next(input);
        let max = self.maximum.next(input);
        if min == max {
            50.0
        } else {
            (input - min) / (max - min) * 100.0
        }
    }
}

impl<'a, T: High + Low + Close> Next<&'a T> for FastStochastic {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        let highest = self.maximum.next(input.high());
        let lowest = self.minimum.next(input.low());
        let close = input.close();
        if highest == lowest {
            50.0
        } else {
            (close - lowest) / (highest - lowest) * 100.0
        }
    }
}

impl Reset for FastStochastic {
    fn reset(&mut self) {
        self.minimum.reset();
        self.maximum.reset();
    }
}

impl Default for FastStochastic {
    fn default() -> Self {
        Self::new(14).unwrap()
    }
}

impl fmt::Display for FastStochastic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FAST_STOCH({})", self.length)
    }
}
