use serde::{Deserialize, Serialize};
use chrono::{DateTime, NaiveDate, Utc};
use super::daily::InstrumentType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MinuteBar {
    pub datetime: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trading_date: Option<NaiveDate>,
    pub order_book_id: String,
    pub instrument_type: InstrumentType,
    pub open: f32,
    pub high: f32,
    pub low: f32,
    pub close: f32,
    pub volume: f32,
    pub total_turnover: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_interest: Option<f32>,
}

pub trait MinuteMarketData {
    fn get_instrument_id(&self) -> &str;
    fn get_datetime(&self) -> DateTime<Utc>;
    fn get_trading_date(&self) -> Option<NaiveDate>;
    fn get_open(&self) -> f32;
    fn get_high(&self) -> f32;
    fn get_low(&self) -> f32;
    fn get_close(&self) -> f32;
    fn get_volume(&self) -> f32;
    fn get_total_turnover(&self) -> f32;
}

impl MinuteMarketData for MinuteBar {
    fn get_instrument_id(&self) -> &str { &self.order_book_id }
    fn get_datetime(&self) -> DateTime<Utc> { self.datetime }
    fn get_trading_date(&self) -> Option<NaiveDate> { self.trading_date }
    fn get_open(&self) -> f32 { self.open }
    fn get_high(&self) -> f32 { self.high }
    fn get_low(&self) -> f32 { self.low }
    fn get_close(&self) -> f32 { self.close }
    fn get_volume(&self) -> f32 { self.volume }
    fn get_total_turnover(&self) -> f32 { self.total_turnover }
}

impl MinuteBar {
    pub fn new(
        datetime: DateTime<Utc>,
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
            datetime, trading_date: None, order_book_id, instrument_type,
            open, high, low, close, volume, total_turnover, open_interest: None,
        }
    }

    pub fn is_stock(&self) -> bool { self.instrument_type == InstrumentType::Stock }
    pub fn is_future(&self) -> bool { self.instrument_type == InstrumentType::Future }
    pub fn is_index(&self) -> bool { self.instrument_type == InstrumentType::Index }
    pub fn is_fund(&self) -> bool { self.instrument_type == InstrumentType::Fund }
    pub fn open_interest(&self) -> Option<f32> { self.open_interest }
    pub fn range(&self) -> f32 { self.high - self.low }
    pub fn returns(&self) -> f32 {
        if self.open == 0.0 { 0.0 } else { (self.close - self.open) / self.open }
    }
}
