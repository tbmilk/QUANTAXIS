use std::fmt;
use crate::qaindicator::errors::Result;
use crate::qaindicator::indicators::ExponentialMovingAverage as Ema;
use crate::qaindicator::{Close, Next, Reset};

#[derive(Debug, Clone)]
pub struct RelativeStrengthIndex {
    n: u32,
    up_ema_indicator: Ema,
    down_ema_indicator: Ema,
    prev_val: f64,
    is_new: bool,
}

impl RelativeStrengthIndex {
    pub fn new(n: u32) -> Result<Self> {
        Ok(Self {
            n,
            up_ema_indicator: Ema::new(n)?,
            down_ema_indicator: Ema::new(n)?,
            prev_val: 0.0,
            is_new: true,
        })
    }
}

impl Next<f64> for RelativeStrengthIndex {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        let mut up = 0.0;
        let mut down = 0.0;

        if self.is_new {
            self.is_new = false;
            up = 0.1;
            down = 0.1;
        } else {
            if input > self.prev_val {
                up = input - self.prev_val;
            } else {
                down = self.prev_val - input;
            }
        }

        self.prev_val = input;
        let up_ema = self.up_ema_indicator.next(up);
        let down_ema = self.down_ema_indicator.next(down);
        100.0 * up_ema / (up_ema + down_ema)
    }
}

impl<'a, T: Close> Next<&'a T> for RelativeStrengthIndex {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.next(input.close())
    }
}

impl Reset for RelativeStrengthIndex {
    fn reset(&mut self) {
        self.is_new = true;
        self.prev_val = 0.0;
        self.up_ema_indicator.reset();
        self.down_ema_indicator.reset();
    }
}

impl Default for RelativeStrengthIndex {
    fn default() -> Self {
        Self::new(14).unwrap()
    }
}

impl fmt::Display for RelativeStrengthIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RSI({})", self.n)
    }
}
