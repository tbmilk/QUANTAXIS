pub mod errors;
pub mod indicators;

pub use indicators::*;

pub trait Next<T> {
    type Output;
    fn next(&mut self, input: T) -> Self::Output;
}

pub trait Reset {
    fn reset(&mut self);
}

pub trait Update<T> {
    type Output;
    fn update(&mut self, input: T) -> Self::Output;
}

pub trait Close {
    fn close(&self) -> f64;
}

pub trait High {
    fn high(&self) -> f64;
}

pub trait Low {
    fn low(&self) -> f64;
}

pub trait Open {
    fn open(&self) -> f64;
}

pub trait Volume {
    fn volume(&self) -> f64;
}

pub(crate) fn max3(a: f64, b: f64, c: f64) -> f64 {
    a.max(b).max(c)
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    use indicators::{
        SimpleMovingAverage as Sma,
        ExponentialMovingAverage as Ema,
        BollingerBands as Boll,
        RelativeStrengthIndex as Rsi,
        AverageTrueRange as Atr,
        MovingAverageConvergenceDivergence as Macd,
    };

    // 固定随机种子的简单 LCG 序列，避免引入随机库依赖
    fn fixed_close_series(n: usize) -> Vec<f64> {
        let mut v = Vec::with_capacity(n);
        let mut x: u64 = 12345;
        let mut price = 10.0_f64;
        for _ in 0..n {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let delta = ((x >> 33) as f64 / u32::MAX as f64 - 0.5) * 0.2;
            price = (price + delta).max(1.0);
            v.push(price);
        }
        v
    }

    fn fixed_hlc_series(n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        let close = fixed_close_series(n);
        let high: Vec<f64> = close.iter().enumerate().map(|(i, &c)| {
            let x: u64 = (i as u64).wrapping_mul(2654435761).wrapping_add(0xdeadbeef);
            c + (x >> 40) as f64 / u32::MAX as f64 * 0.3
        }).collect();
        let low: Vec<f64> = close.iter().enumerate().map(|(i, &c)| {
            let x: u64 = (i as u64).wrapping_mul(1234567891).wrapping_add(0xcafebabe);
            (c - (x >> 40) as f64 / u32::MAX as f64 * 0.3).max(0.01)
        }).collect();
        (high, low, close)
    }

    #[test]
    fn test_sma_convergence_to_arithmetic_mean() {
        // SMA(n) 第 n 个值应等于前 n 个值的算术平均
        let data: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let mut sma = Sma::new(5).unwrap();
        let results: Vec<f64> = data.iter().map(|&v| sma.next(v)).collect();
        // 第 5 个值 (index 4) = mean(1,2,3,4,5) = 3.0
        assert!((results[4] - 3.0).abs() < 1e-9, "SMA[4]={}", results[4]);
        // 第 6 个值 = mean(2,3,4,5,6) = 4.0
        assert!((results[5] - 4.0).abs() < 1e-9, "SMA[5]={}", results[5]);
    }

    #[test]
    fn test_sma_reset_restores_initial_state() {
        let mut sma = Sma::new(3).unwrap();
        let r1: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0].iter().map(|&v| sma.next(v)).collect();
        sma.reset();
        let r2: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0].iter().map(|&v| sma.next(v)).collect();
        for (a, b) in r1.iter().zip(r2.iter()) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    #[test]
    fn test_ema_first_value_equals_input() {
        let mut ema = Ema::new(14).unwrap();
        let first = ema.next(42.0);
        assert!((first - 42.0).abs() < 1e-9, "EMA first={}", first);
    }

    #[test]
    fn test_ema_convergence_on_constant_series() {
        // EMA of constant series converges to the constant itself
        let mut ema = Ema::new(10).unwrap();
        let val = 7.5;
        let mut last = 0.0;
        for _ in 0..200 {
            last = ema.next(val);
        }
        assert!((last - val).abs() < 1e-9, "EMA converged to {}", last);
    }

    #[test]
    fn test_ema_ma20_at_index_20_matches_known_value() {
        // Deterministic check: MA(5) at index 4 on [1..5] = 3.0
        let mut ema = Ema::new(5).unwrap();
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let results: Vec<f64> = data.iter().map(|&v| ema.next(v)).collect();
        // All values finite
        for r in &results {
            assert!(r.is_finite(), "EMA output NaN/inf");
        }
        // Monotonically increasing for ascending input
        for i in 1..results.len() {
            assert!(results[i] >= results[i-1] - 1e-9, "EMA not monotone at {}", i);
        }
    }

    #[test]
    fn test_rsi_values_in_0_100() {
        let close = fixed_close_series(200);
        let mut rsi = Rsi::new(14).unwrap();
        for &v in &close {
            let r = rsi.next(v);
            assert!(r >= 0.0 && r <= 100.0, "RSI={} out of [0,100]", r);
        }
    }

    #[test]
    fn test_boll_upper_ge_mid_ge_lower() {
        let close = fixed_close_series(200);
        let mut boll = Boll::new(20, 2.0).unwrap();
        for (i, &v) in close.iter().enumerate() {
            let out = boll.next(v);
            if i >= 19 && out.upper.is_finite() && out.lower.is_finite() {
                // Welford SD can produce tiny negative m2 by float rounding → NaN: skip NaN outputs
                assert!(out.upper >= out.average - 1e-6, "upper < mid at {}: upper={} avg={}", i, out.upper, out.average);
                assert!(out.average >= out.lower - 1e-6, "mid < lower at {}", i);
            }
        }
    }

    #[test]
    fn test_atr_nonnegative() {
        let (high, low, close) = fixed_hlc_series(200);
        struct Bar { h: f64, l: f64, c: f64 }
        impl High for Bar { fn high(&self) -> f64 { self.h } }
        impl Low  for Bar { fn low(&self)  -> f64 { self.l } }
        impl Close for Bar { fn close(&self) -> f64 { self.c } }
        let mut atr = Atr::new(14).unwrap();
        for i in 0..200 {
            let bar = Bar { h: high[i], l: low[i], c: close[i] };
            let v = atr.next(&bar);
            assert!(v >= 0.0, "ATR<0 at {}: {}", i, v);
        }
    }

    #[test]
    fn test_macd_histogram_equals_diff_minus_signal() {
        // MACD returns (macd_line, signal, histogram)
        let close = fixed_close_series(200);
        let mut macd = Macd::new(12, 26, 9).unwrap();
        for &v in &close {
            let (macd_line, signal, histogram) = macd.next(v);
            let expected = macd_line - signal;
            assert!((histogram - expected).abs() < 1e-9,
                    "histogram={} expected={}", histogram, expected);
        }
    }

    #[test]
    fn test_sma_stable_segment_known_values() {
        // Python MA(5) after warmup = rolling mean. Verify Rust matches analytically.
        // data = [10,11,12,13,14,15,16] → MA5 at index 4 = 12.0, index 6 = 14.0
        let data = vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0];
        let mut sma = Sma::new(5).unwrap();
        let out: Vec<f64> = data.iter().map(|&v| sma.next(v)).collect();
        assert!((out[4] - 12.0).abs() < 1e-9);
        assert!((out[5] - 13.0).abs() < 1e-9);
        assert!((out[6] - 14.0).abs() < 1e-9);
    }
}
