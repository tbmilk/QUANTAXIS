use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use super::snapshot::MDSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tick {
    pub instrument_id: String,
    pub last_price: f64,
    pub volume: i64,
    pub amount: f64,
    pub datetime: DateTime<Utc>,
}

impl Tick {
    pub fn new(
        instrument_id: String,
        last_price: f64,
        volume: i64,
        amount: f64,
        datetime: DateTime<Utc>,
    ) -> Self {
        Self { instrument_id, last_price, volume, amount, datetime }
    }

    pub fn from_snapshot(snapshot: &MDSnapshot) -> Self {
        Self {
            instrument_id: snapshot.instrument_id.clone(),
            last_price: snapshot.last_price,
            volume: snapshot.volume,
            amount: snapshot.amount,
            datetime: snapshot.datetime,
        }
    }
}
