use std::fmt;
use crate::qaindicator::{Close, Next, Reset, Volume};

#[derive(Debug, Clone)]
pub struct OnBalanceVolume {
    obv: f64,
    prev_close: f64,
}

impl OnBalanceVolume {
    pub fn new() -> Self {
        Self { obv: 0.0, prev_close: 0.0 }
    }
}

impl<'a, T: Close + Volume> Next<&'a T> for OnBalanceVolume {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> f64 {
        if input.close() > self.prev_close {
            self.obv = self.obv + input.volume();
        } else if input.close() < self.prev_close {
            self.obv = self.obv - input.volume();
        }
        self.prev_close = input.close();
        self.obv
    }
}

impl Default for OnBalanceVolume {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OnBalanceVolume {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "OBV")
    }
}

impl Reset for OnBalanceVolume {
    fn reset(&mut self) {
        self.obv = 0.0;
        self.prev_close = 0.0;
    }
}
