use std::fmt;
use crate::qaindicator::errors::Result;
use crate::qaindicator::indicators::ExponentialMovingAverage as Ema;
use crate::qaindicator::{Close, Next, Reset};

#[derive(Debug, Clone)]
pub struct MovingAverageConvergenceDivergence {
    fast_ema: Ema,
    slow_ema: Ema,
    signal_ema: Ema,
}

impl MovingAverageConvergenceDivergence {
    pub fn new(fast_length: u32, slow_length: u32, signal_length: u32) -> Result<Self> {
        Ok(Self {
            fast_ema: Ema::new(fast_length)?,
            slow_ema: Ema::new(slow_length)?,
            signal_ema: Ema::new(signal_length)?,
        })
    }
}

impl Next<f64> for MovingAverageConvergenceDivergence {
    type Output = (f64, f64, f64);

    fn next(&mut self, input: f64) -> Self::Output {
        let fast_val = self.fast_ema.next(input);
        let slow_val = self.slow_ema.next(input);
        let macd = fast_val - slow_val;
        let signal = self.signal_ema.next(macd);
        let histogram = macd - signal;
        (macd, signal, histogram)
    }
}

impl<'a, T: Close> Next<&'a T> for MovingAverageConvergenceDivergence {
    type Output = (f64, f64, f64);

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.next(input.close())
    }
}

impl Reset for MovingAverageConvergenceDivergence {
    fn reset(&mut self) {
        self.fast_ema.reset();
        self.slow_ema.reset();
        self.signal_ema.reset();
    }
}

impl Default for MovingAverageConvergenceDivergence {
    fn default() -> Self {
        Self::new(12, 26, 9).unwrap()
    }
}

impl fmt::Display for MovingAverageConvergenceDivergence {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MACD({}, {}, {})",
            self.fast_ema.length(),
            self.slow_ema.length(),
            self.signal_ema.length()
        )
    }
}
