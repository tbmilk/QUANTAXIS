use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::indicators::StandardDeviation as Sd;
use crate::qaindicator::{Close, Next, Reset};

#[derive(Debug, Clone)]
pub struct BollingerBands {
    length: u32,
    multiplier: f64,
    sd: Sd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BollingerBandsOutput {
    pub average: f64,
    pub upper: f64,
    pub lower: f64,
}

impl BollingerBands {
    pub fn new(length: u32, multiplier: f64) -> Result<Self> {
        if multiplier <= 0.0 {
            return Err(IndicatorError::InvalidParameter);
        }
        Ok(Self {
            length,
            multiplier,
            sd: Sd::new(length)?,
        })
    }

    pub fn length(&self) -> u32 {
        self.length
    }

    pub fn multiplier(&self) -> f64 {
        self.multiplier
    }
}

impl Next<f64> for BollingerBands {
    type Output = BollingerBandsOutput;

    fn next(&mut self, input: f64) -> Self::Output {
        let sd = self.sd.next(input);
        let mean = self.sd.mean();
        Self::Output {
            average: mean,
            upper: mean + sd * self.multiplier,
            lower: mean - sd * self.multiplier,
        }
    }
}

impl<'a, T: Close> Next<&'a T> for BollingerBands {
    type Output = BollingerBandsOutput;

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.next(input.close())
    }
}

impl Reset for BollingerBands {
    fn reset(&mut self) {
        self.sd.reset();
    }
}

impl Default for BollingerBands {
    fn default() -> Self {
        Self::new(9, 2_f64).unwrap()
    }
}

impl fmt::Display for BollingerBands {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BB({}, {})", self.length, self.multiplier)
    }
}
