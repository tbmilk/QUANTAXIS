use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    buffer: VecDeque<T>,
    capacity: usize,
}

impl<T: Clone> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self { buffer: VecDeque::with_capacity(capacity), capacity }
    }

    pub fn push(&mut self, value: T) -> Option<T> {
        let expired = if self.buffer.len() >= self.capacity {
            self.buffer.pop_front()
        } else {
            None
        };
        self.buffer.push_back(value);
        expired
    }

    pub fn latest(&self) -> Option<&T> { self.buffer.back() }
    pub fn oldest(&self) -> Option<&T> { self.buffer.front() }
    pub fn get(&self, index: usize) -> Option<&T> { self.buffer.get(index) }

    pub fn get_from_back(&self, n: usize) -> Option<&T> {
        if n >= self.buffer.len() { None }
        else { self.buffer.get(self.buffer.len() - 1 - n) }
    }

    pub fn len(&self) -> usize { self.buffer.len() }
    pub fn is_empty(&self) -> bool { self.buffer.is_empty() }
    pub fn is_full(&self) -> bool { self.buffer.len() >= self.capacity }
    pub fn capacity(&self) -> usize { self.capacity }
    pub fn clear(&mut self) { self.buffer.clear(); }
    pub fn iter(&self) -> impl Iterator<Item = &T> { self.buffer.iter() }
    pub fn iter_rev(&self) -> impl Iterator<Item = &T> { self.buffer.iter().rev() }
    pub fn to_vec(&self) -> Vec<T> { self.buffer.iter().cloned().collect() }
}

impl<T: Clone> Default for RingBuffer<T> {
    fn default() -> Self { Self::new(64) }
}

#[derive(Debug, Clone)]
pub struct NumericRingBuffer {
    buffer: RingBuffer<f64>,
    sum: f64,
}

impl NumericRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self { buffer: RingBuffer::new(capacity), sum: 0.0 }
    }

    pub fn push(&mut self, value: f64) -> Option<f64> {
        let expired = self.buffer.push(value);
        self.sum += value;
        if let Some(exp) = expired { self.sum -= exp; }
        expired
    }

    pub fn sum(&self) -> f64 { self.sum }
    pub fn mean(&self) -> f64 {
        if self.buffer.is_empty() { 0.0 } else { self.sum / self.buffer.len() as f64 }
    }
    pub fn len(&self) -> usize { self.buffer.len() }
    pub fn is_empty(&self) -> bool { self.buffer.is_empty() }
    pub fn is_full(&self) -> bool { self.buffer.is_full() }
    pub fn capacity(&self) -> usize { self.buffer.capacity() }
    pub fn latest(&self) -> Option<f64> { self.buffer.latest().copied() }
    pub fn oldest(&self) -> Option<f64> { self.buffer.oldest().copied() }
    pub fn values(&self) -> Vec<f64> { self.buffer.to_vec() }
    pub fn clear(&mut self) { self.buffer.clear(); self.sum = 0.0; }
    pub fn iter(&self) -> impl Iterator<Item = &f64> { self.buffer.iter() }
}

impl Default for NumericRingBuffer {
    fn default() -> Self { Self::new(64) }
}

#[derive(Debug, Clone)]
pub struct PairedRingBuffer {
    x_buffer: RingBuffer<f64>,
    y_buffer: RingBuffer<f64>,
    sum_x: f64,
    sum_y: f64,
    sum_xy: f64,
    sum_x2: f64,
    sum_y2: f64,
}

impl PairedRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            x_buffer: RingBuffer::new(capacity),
            y_buffer: RingBuffer::new(capacity),
            sum_x: 0.0, sum_y: 0.0, sum_xy: 0.0, sum_x2: 0.0, sum_y2: 0.0,
        }
    }

    pub fn push(&mut self, x: f64, y: f64) -> Option<(f64, f64)> {
        let exp_x = self.x_buffer.push(x);
        let exp_y = self.y_buffer.push(y);
        self.sum_x += x; self.sum_y += y;
        self.sum_xy += x * y; self.sum_x2 += x * x; self.sum_y2 += y * y;
        if let (Some(ex), Some(ey)) = (exp_x, exp_y) {
            self.sum_x -= ex; self.sum_y -= ey;
            self.sum_xy -= ex * ey; self.sum_x2 -= ex * ex; self.sum_y2 -= ey * ey;
            Some((ex, ey))
        } else {
            None
        }
    }

    pub fn covariance(&self) -> f64 {
        let n = self.len() as f64;
        if n < 2.0 { return 0.0; }
        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;
        (self.sum_xy / n) - (mean_x * mean_y)
    }

    pub fn correlation(&self) -> f64 {
        let n = self.len() as f64;
        if n < 2.0 { return 0.0; }
        let mean_x = self.sum_x / n;
        let mean_y = self.sum_y / n;
        let var_x = (self.sum_x2 / n) - (mean_x * mean_x);
        let var_y = (self.sum_y2 / n) - (mean_y * mean_y);
        if var_x <= 0.0 || var_y <= 0.0 { return 0.0; }
        let cov = (self.sum_xy / n) - (mean_x * mean_y);
        cov / (var_x.sqrt() * var_y.sqrt())
    }

    pub fn len(&self) -> usize { self.x_buffer.len() }
    pub fn is_empty(&self) -> bool { self.x_buffer.is_empty() }
    pub fn is_full(&self) -> bool { self.x_buffer.is_full() }
    pub fn capacity(&self) -> usize { self.x_buffer.capacity() }

    pub fn clear(&mut self) {
        self.x_buffer.clear(); self.y_buffer.clear();
        self.sum_x = 0.0; self.sum_y = 0.0;
        self.sum_xy = 0.0; self.sum_x2 = 0.0; self.sum_y2 = 0.0;
    }
}
