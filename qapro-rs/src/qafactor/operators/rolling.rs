use super::ring_buffer::{NumericRingBuffer, PairedRingBuffer};
use super::welford::WindowedWelfordState;

#[derive(Debug, Clone)]
pub struct RollingMean {
    buffer: NumericRingBuffer,
}

impl RollingMean {
    pub fn new(window_size: usize) -> Self {
        Self { buffer: NumericRingBuffer::new(window_size) }
    }

    pub fn update(&mut self, value: f64) { self.buffer.push(value); }
    pub fn value(&self) -> f64 { self.buffer.mean() }
    pub fn sum(&self) -> f64 { self.buffer.sum() }
    pub fn count(&self) -> usize { self.buffer.len() }
    pub fn is_full(&self) -> bool { self.buffer.is_full() }
    pub fn reset(&mut self) { self.buffer.clear(); }
}

impl Default for RollingMean {
    fn default() -> Self { Self::new(20) }
}

#[derive(Debug, Clone)]
pub struct RollingStd {
    welford: WindowedWelfordState,
}

impl RollingStd {
    pub fn new(window_size: usize) -> Self {
        Self { welford: WindowedWelfordState::new(window_size) }
    }

    pub fn update(&mut self, value: f64) { self.welford.update(value); }
    pub fn value(&self) -> f64 { self.welford.std() }
    pub fn variance(&self) -> f64 { self.welford.variance() }
    pub fn mean(&self) -> f64 { self.welford.mean() }
    pub fn skewness(&self) -> f64 { self.welford.skewness() }
    pub fn kurtosis(&self) -> f64 { self.welford.kurtosis() }
    pub fn count(&self) -> usize { self.welford.count() }
    pub fn is_full(&self) -> bool { self.welford.is_full() }
    pub fn reset(&mut self) { self.welford.reset(); }
}

impl Default for RollingStd {
    fn default() -> Self { Self::new(20) }
}

#[derive(Debug, Clone)]
pub struct RollingCorr {
    buffer: PairedRingBuffer,
}

impl RollingCorr {
    pub fn new(window_size: usize) -> Self {
        Self { buffer: PairedRingBuffer::new(window_size) }
    }

    pub fn update(&mut self, x: f64, y: f64) { self.buffer.push(x, y); }
    pub fn value(&self) -> f64 { self.buffer.correlation() }
    pub fn covariance(&self) -> f64 { self.buffer.covariance() }
    pub fn count(&self) -> usize { self.buffer.len() }
    pub fn is_full(&self) -> bool { self.buffer.is_full() }
    pub fn reset(&mut self) { self.buffer.clear(); }
}

impl Default for RollingCorr {
    fn default() -> Self { Self::new(20) }
}

#[derive(Debug, Clone)]
pub struct EMA {
    alpha: f64,
    ema: Option<f64>,
    count: u64,
}

impl EMA {
    pub fn new(period: usize) -> Self {
        Self { alpha: 2.0 / (period as f64 + 1.0), ema: None, count: 0 }
    }

    pub fn update(&mut self, price: f64) {
        self.count += 1;
        self.ema = Some(match self.ema {
            None => price,
            Some(prev) => self.alpha * price + (1.0 - self.alpha) * prev,
        });
    }

    pub fn value(&self) -> Option<f64> { self.ema }
    pub fn count(&self) -> u64 { self.count }
    pub fn reset(&mut self) { self.ema = None; self.count = 0; }
}

impl Default for EMA {
    fn default() -> Self { Self::new(9) }
}

#[derive(Debug, Clone)]
pub struct RSI {
    period: usize,
    avg_gain: Option<f64>,
    avg_loss: Option<f64>,
    prev_price: Option<f64>,
    count: u64,
}

impl RSI {
    pub fn new(period: usize) -> Self {
        Self { period, avg_gain: None, avg_loss: None, prev_price: None, count: 0 }
    }

    pub fn update(&mut self, price: f64) {
        if let Some(prev) = self.prev_price {
            let change = price - prev;
            let gain = if change > 0.0 { change } else { 0.0 };
            let loss = if change < 0.0 { -change } else { 0.0 };
            let n = self.period as f64;
            match (self.avg_gain, self.avg_loss) {
                (None, _) | (_, None) => {
                    self.avg_gain = Some(gain);
                    self.avg_loss = Some(loss);
                }
                (Some(ag), Some(al)) => {
                    self.avg_gain = Some((ag * (n - 1.0) + gain) / n);
                    self.avg_loss = Some((al * (n - 1.0) + loss) / n);
                }
            }
        }
        self.prev_price = Some(price);
        self.count += 1;
    }

    pub fn value(&self) -> Option<f64> {
        match (self.avg_gain, self.avg_loss) {
            (Some(ag), Some(al)) => {
                if al == 0.0 { Some(100.0) }
                else { Some(100.0 - 100.0 / (1.0 + ag / al)) }
            }
            _ => None,
        }
    }

    pub fn count(&self) -> u64 { self.count }
    pub fn reset(&mut self) {
        self.avg_gain = None;
        self.avg_loss = None;
        self.prev_price = None;
        self.count = 0;
    }
}

impl Default for RSI {
    fn default() -> Self { Self::new(14) }
}
