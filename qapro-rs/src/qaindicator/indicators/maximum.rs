use std::f64::INFINITY;
use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::{High, Next, Reset};

#[derive(Debug, Clone)]
pub struct Maximum {
    n: usize,
    vec: Vec<f64>,
    max_index: usize,
    cur_index: usize,
}

impl Maximum {
    pub fn new(n: u32) -> Result<Self> {
        let n = n as usize;
        if n == 0 {
            return Err(IndicatorError::InvalidParameter);
        }
        Ok(Self {
            n,
            vec: vec![-INFINITY; n],
            max_index: 0,
            cur_index: 0,
        })
    }

    fn find_max_index(&self) -> usize {
        let mut max = -INFINITY;
        let mut index: usize = 0;
        for (i, &val) in self.vec.iter().enumerate() {
            if val > max {
                max = val;
                index = i;
            }
        }
        index
    }
}

impl Next<f64> for Maximum {
    type Output = f64;

    fn next(&mut self, input: f64) -> Self::Output {
        self.cur_index = (self.cur_index + 1) % self.n;
        self.vec[self.cur_index] = input;
        if input > self.vec[self.max_index] {
            self.max_index = self.cur_index;
        } else if self.max_index == self.cur_index {
            self.max_index = self.find_max_index();
        }
        self.vec[self.max_index]
    }
}

impl<'a, T: High> Next<&'a T> for Maximum {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> Self::Output {
        self.next(input.high())
    }
}

impl Reset for Maximum {
    fn reset(&mut self) {
        for i in 0..self.n {
            self.vec[i] = -INFINITY;
        }
    }
}

impl Default for Maximum {
    fn default() -> Self {
        Self::new(14).unwrap()
    }
}

impl fmt::Display for Maximum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MAX({})", self.n)
    }
}
