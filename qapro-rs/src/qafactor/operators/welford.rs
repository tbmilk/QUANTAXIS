use super::ring_buffer::RingBuffer;

#[derive(Debug, Clone, Default)]
pub struct WelfordState {
    pub count: u64,
    pub mean: f64,
    pub m2: f64,
    pub m3: f64,
    pub m4: f64,
}

impl WelfordState {
    pub fn new() -> Self { Self::default() }

    pub fn update(&mut self, x: f64) {
        self.count += 1;
        let n = self.count as f64;
        let delta = x - self.mean;
        let delta_n = delta / n;
        let delta_n2 = delta_n * delta_n;
        let term1 = delta * delta_n * (n - 1.0);
        self.mean += delta_n;
        self.m4 += term1 * delta_n2 * (n * n - 3.0 * n + 3.0)
            + 6.0 * delta_n2 * self.m2
            - 4.0 * delta_n * self.m3;
        self.m3 += term1 * delta_n * (n - 2.0) - 3.0 * delta_n * self.m2;
        self.m2 += term1;
    }

    pub fn variance(&self) -> f64 {
        if self.count < 2 { 0.0 } else { self.m2 / self.count as f64 }
    }

    pub fn sample_variance(&self) -> f64 {
        if self.count < 2 { 0.0 } else { self.m2 / (self.count - 1) as f64 }
    }

    pub fn std(&self) -> f64 { self.variance().sqrt() }
    pub fn sample_std(&self) -> f64 { self.sample_variance().sqrt() }

    pub fn skewness(&self) -> f64 {
        if self.count < 3 || self.m2 == 0.0 { 0.0 }
        else { (self.count as f64).sqrt() * self.m3 / self.m2.powf(1.5) }
    }

    pub fn kurtosis(&self) -> f64 {
        if self.count < 4 || self.m2 == 0.0 { 0.0 }
        else {
            let n = self.count as f64;
            (n * self.m4) / (self.m2 * self.m2) - 3.0
        }
    }

    pub fn merge(&self, other: &WelfordState) -> WelfordState {
        if self.count == 0 { return other.clone(); }
        if other.count == 0 { return self.clone(); }
        let combined_count = self.count + other.count;
        let delta = other.mean - self.mean;
        let delta2 = delta * delta;
        let delta3 = delta * delta2;
        let delta4 = delta2 * delta2;
        let n1 = self.count as f64;
        let n2 = other.count as f64;
        let n = combined_count as f64;
        let combined_mean = (self.mean * n1 + other.mean * n2) / n;
        let combined_m2 = self.m2 + other.m2 + delta2 * n1 * n2 / n;
        let combined_m3 = self.m3 + other.m3
            + delta3 * n1 * n2 * (n1 - n2) / (n * n)
            + 3.0 * delta * (n1 * other.m2 - n2 * self.m2) / n;
        let combined_m4 = self.m4 + other.m4
            + delta4 * n1 * n2 * (n1 * n1 - n1 * n2 + n2 * n2) / (n * n * n)
            + 6.0 * delta2 * (n1 * n1 * other.m2 + n2 * n2 * self.m2) / (n * n)
            + 4.0 * delta * (n1 * other.m3 - n2 * self.m3) / n;
        WelfordState {
            count: combined_count, mean: combined_mean,
            m2: combined_m2, m3: combined_m3, m4: combined_m4,
        }
    }

    pub fn reset(&mut self) { *self = Self::default(); }
}

#[derive(Debug, Clone)]
pub struct WindowedWelfordState {
    window_size: usize,
    buffer: RingBuffer<f64>,
    state: WelfordState,
}

impl WindowedWelfordState {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            buffer: RingBuffer::new(window_size),
            state: WelfordState::new(),
        }
    }

    pub fn update(&mut self, x: f64) {
        if self.buffer.is_full() {
            let expired = self.buffer.push(x);
            if expired.is_some() {
                self.recalculate();
            }
        } else {
            self.buffer.push(x);
            self.state.update(x);
        }
    }

    fn recalculate(&mut self) {
        self.state = WelfordState::new();
        for &val in self.buffer.iter() {
            self.state.update(val);
        }
    }

    pub fn mean(&self) -> f64 { self.state.mean }
    pub fn variance(&self) -> f64 { self.state.variance() }
    pub fn std(&self) -> f64 { self.state.std() }
    pub fn skewness(&self) -> f64 { self.state.skewness() }
    pub fn kurtosis(&self) -> f64 { self.state.kurtosis() }
    pub fn count(&self) -> usize { self.buffer.len() }
    pub fn is_full(&self) -> bool { self.buffer.is_full() }
    pub fn window_size(&self) -> usize { self.window_size }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.state.reset();
    }
}

impl Default for WindowedWelfordState {
    fn default() -> Self { Self::new(20) }
}

#[derive(Debug, Clone, Default)]
pub struct WelfordCovarianceState {
    pub count: u64,
    pub mean_x: f64,
    pub mean_y: f64,
    pub m2_x: f64,
    pub m2_y: f64,
    pub c: f64,
}

impl WelfordCovarianceState {
    pub fn new() -> Self { Self::default() }

    pub fn update(&mut self, x: f64, y: f64) {
        self.count += 1;
        let n = self.count as f64;
        let dx = x - self.mean_x;
        let dy = y - self.mean_y;
        self.mean_x += dx / n;
        self.mean_y += dy / n;
        let dx2 = x - self.mean_x;
        let dy2 = y - self.mean_y;
        self.m2_x += dx * dx2;
        self.m2_y += dy * dy2;
        self.c += dx * dy2;
    }

    pub fn covariance(&self) -> f64 {
        if self.count < 2 { 0.0 } else { self.c / self.count as f64 }
    }

    pub fn sample_covariance(&self) -> f64 {
        if self.count < 2 { 0.0 } else { self.c / (self.count - 1) as f64 }
    }

    pub fn correlation(&self) -> f64 {
        if self.count < 2 || self.m2_x <= 0.0 || self.m2_y <= 0.0 { 0.0 }
        else { self.c / (self.m2_x.sqrt() * self.m2_y.sqrt()) }
    }

    pub fn std_x(&self) -> f64 {
        if self.count < 2 { 0.0 } else { (self.m2_x / self.count as f64).sqrt() }
    }

    pub fn std_y(&self) -> f64 {
        if self.count < 2 { 0.0 } else { (self.m2_y / self.count as f64).sqrt() }
    }

    pub fn reset(&mut self) { *self = Self::default(); }
}
