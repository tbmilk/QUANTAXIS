use serde::{Deserialize, Serialize};
use chrono::NaiveDate;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstrumentType {
    Stock,
    Future,
    Index,
    Fund,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DailyBar {
    pub date: NaiveDate,
    pub order_book_id: String,
    pub instrument_type: InstrumentType,
    pub open: f32,
    pub high: f32,
    pub low: f32,
    pub close: f32,
    pub volume: f32,
    pub total_turnover: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_trades: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_up: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_down: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_interest: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_settlement: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iopv: Option<f32>,
}

pub trait DailyMarketData {
    fn get_instrument_id(&self) -> &str;
    fn get_date(&self) -> NaiveDate;
    fn get_open(&self) -> f32;
    fn get_high(&self) -> f32;
    fn get_low(&self) -> f32;
    fn get_close(&self) -> f32;
    fn get_volume(&self) -> f32;
    fn get_total_turnover(&self) -> f32;
}

impl DailyMarketData for DailyBar {
    fn get_instrument_id(&self) -> &str { &self.order_book_id }
    fn get_date(&self) -> NaiveDate { self.date }
    fn get_open(&self) -> f32 { self.open }
    fn get_high(&self) -> f32 { self.high }
    fn get_low(&self) -> f32 { self.low }
    fn get_close(&self) -> f32 { self.close }
    fn get_volume(&self) -> f32 { self.volume }
    fn get_total_turnover(&self) -> f32 { self.total_turnover }
}

impl DailyBar {
    pub fn new(
        date: NaiveDate,
        order_book_id: String,
        instrument_type: InstrumentType,
        open: f32,
        high: f32,
        low: f32,
        close: f32,
        volume: f32,
        total_turnover: f32,
    ) -> Self {
        Self {
            date, order_book_id, instrument_type,
            open, high, low, close, volume, total_turnover,
            num_trades: None, limit_up: None, limit_down: None,
            open_interest: None, prev_settlement: None, settlement: None, iopv: None,
        }
    }

    pub fn is_stock(&self) -> bool { self.instrument_type == InstrumentType::Stock }
    pub fn is_future(&self) -> bool { self.instrument_type == InstrumentType::Future }
    pub fn is_index(&self) -> bool { self.instrument_type == InstrumentType::Index }
    pub fn is_fund(&self) -> bool { self.instrument_type == InstrumentType::Fund }
    pub fn open_interest(&self) -> Option<f32> { self.open_interest }
    pub fn settlement(&self) -> Option<f32> { self.settlement }
    pub fn prev_settlement(&self) -> Option<f32> { self.prev_settlement }
    pub fn iopv(&self) -> Option<f32> { self.iopv }
    pub fn num_trades(&self) -> Option<f32> { self.num_trades }
    pub fn price_limits(&self) -> Option<(f32, f32)> {
        match (self.limit_down, self.limit_up) {
            (Some(down), Some(up)) => Some((down, up)),
            _ => None,
        }
    }
}
