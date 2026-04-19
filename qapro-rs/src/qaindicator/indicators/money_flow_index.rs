use std::collections::VecDeque;
use std::fmt;
use crate::qaindicator::errors::{IndicatorError, Result};
use crate::qaindicator::{Close, High, Low, Next, Reset, Volume};

#[derive(Debug, Clone)]
pub struct MoneyFlowIndex {
    n: u32,
    money_flows: VecDeque<f64>,
    prev_typical_price: f64,
    total_positive_money_flow: f64,
    total_absolute_money_flow: f64,
    is_new: bool,
}

impl MoneyFlowIndex {
    pub fn new(n: u32) -> Result<Self> {
        match n {
            0 => Err(IndicatorError::InvalidParameter),
            _ => Ok(Self {
                n,
                money_flows: VecDeque::with_capacity(n as usize + 1),
                prev_typical_price: 0.0,
                total_positive_money_flow: 0.0,
                total_absolute_money_flow: 0.0,
                is_new: true,
            }),
        }
    }
}

impl<'a, T: High + Low + Close + Volume> Next<&'a T> for MoneyFlowIndex {
    type Output = f64;

    fn next(&mut self, input: &'a T) -> f64 {
        let typical_price = (input.high() + input.low() + input.close()) / 3.0;

        if self.is_new {
            self.money_flows.push_back(0.0);
            self.prev_typical_price = typical_price;
            self.is_new = false;
            return 50.0;
        }

        let money_flow = typical_price * input.volume();
        let signed_money_flow = if typical_price >= self.prev_typical_price {
            self.total_positive_money_flow += money_flow;
            money_flow
        } else {
            -money_flow
        };
        self.total_absolute_money_flow += money_flow;

        if self.money_flows.len() == (self.n as usize) {
            let old = self.money_flows.pop_front().unwrap();
            if old > 0.0 {
                self.total_positive_money_flow -= old;
                self.total_absolute_money_flow -= old;
            } else {
                self.total_absolute_money_flow += old;
            }
        }

        self.money_flows.push_back(signed_money_flow);
        self.prev_typical_price = typical_price;
        (self.total_positive_money_flow / self.total_absolute_money_flow) * 100.0
    }
}

impl Default for MoneyFlowIndex {
    fn default() -> Self {
        Self::new(14).unwrap()
    }
}

impl fmt::Display for MoneyFlowIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MFI({})", self.n)
    }
}

impl Reset for MoneyFlowIndex {
    fn reset(&mut self) {
        self.money_flows.clear();
        self.prev_typical_price = 0.0;
        self.total_positive_money_flow = 0.0;
        self.total_absolute_money_flow = 0.0;
        self.is_new = true;
    }
}
