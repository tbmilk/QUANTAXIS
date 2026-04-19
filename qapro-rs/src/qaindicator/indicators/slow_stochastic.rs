use std::fmt;
use crate::qaindicator::errors::Result;
use crate::qaindicator::indicators::{ExponentialMovingAverage, FastStochastic};
use crate::qaindicator::{Close, High, Low, Next, Reset};

#[derive(Clone, Debug)]
pub struct SlowStochastic {
    fast_stochastic: FastStochastic,
    ema: ExponentialMovingAverage,
}

impl SlowStochastic {
    pub fn new(stochastic_n: u32, ema_n: u32) -> Result<Self> {
        Ok(Self {
            fast_stochastic: FastStochastic::new(stochastic_n)?,
            ema: ExponentialMovingAverage::new(ema_n)?,
        })
    }
}

impl Next<f64> for SlowStochastic {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        self.ema.next(self.fast_stochastic.next(input))
    }
}

impl<'a, T: High + Low + Close> Next<&'a T> for SlowStochastic {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.ema.next(self.fast_stochastic.next(input))
    }
}

impl Reset for SlowStochastic {
    fn reset(&mut self) {
        self.fast_stochastic.reset();
        self.ema.reset();
    }
}

impl Default for SlowStochastic {
    fn default() -> Self {
        Self::new(14, 3).unwrap()
    }
}

impl fmt::Display for SlowStochastic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "SLOW_STOCH({}, {})",
            self.fast_stochastic.length(),
            self.ema.length()
        )
    }
}
