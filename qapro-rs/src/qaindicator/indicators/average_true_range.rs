#![allow(dead_code)]
use std::fmt;
use crate::qaindicator::errors::Result;
use crate::qaindicator::indicators::{MovingAverage, TrueRange};
use crate::qaindicator::{Close, High, Low, Next, Reset, Update};
use std::f64::INFINITY;

#[derive(Debug, Clone)]
pub struct AverageTrueRange {
    true_range: TrueRange,
    ma: MovingAverage,
    length: usize,
    pub cached: Vec<f64>,
}

impl AverageTrueRange {
    pub fn new(length: u32) -> Result<Self> {
        Ok(Self {
            true_range: TrueRange::new(),
            ma: MovingAverage::new(length)?,
            length: length as usize,
            cached: vec![-INFINITY; length as usize],
        })
    }
}

impl Next<f64> for AverageTrueRange {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        let res = self.ma.next(self.true_range.next(input));
        self.cached.push(res);
        self.cached.remove(0);
        res
    }
}

impl Update<f64> for AverageTrueRange {
    type Output = f64;

    fn update(&mut self, input: f64) -> Self::Output {
        let res = self.ma.update(self.true_range.update(input));
        let x = self.cached.last_mut().unwrap();
        *x = res;
        res
    }
}

impl<'a, T: High + Low + Close> Next<&'a T> for AverageTrueRange {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        let res = self.ma.next(self.true_range.next(input));
        self.cached.push(res);
        self.cached.remove(0);
        res
    }
}

impl<'a, T: High + Low + Close> Update<&'a T> for AverageTrueRange {
    type Output = f64;

    fn update(&mut self, input: &'a T) -> Self::Output {
        let res = self.ma.update(self.true_range.update(input));
        let x = self.cached.last_mut().unwrap();
        *x = res;
        res
    }
}

impl Reset for AverageTrueRange {
    fn reset(&mut self) {
        self.true_range.reset();
        self.ma.reset();
    }
}

impl Default for AverageTrueRange {
    fn default() -> Self {
        Self::new(14).unwrap()
    }
}

impl fmt::Display for AverageTrueRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ATR({})", self.ma.n)
    }
}
