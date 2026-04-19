use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// 可选数值类型，用于市场数据中可能为"-"或null的字段
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionalNumeric<T> {
    Value(T),
    String(String),
    #[serde(rename = "null")]
    Null,
}

impl<T: Default> Default for OptionalNumeric<T> {
    fn default() -> Self {
        OptionalNumeric::Null
    }
}

impl<T: fmt::Display> fmt::Display for OptionalNumeric<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OptionalNumeric::Value(v) => write!(f, "{}", v),
            OptionalNumeric::String(s) => write!(f, "{}", s),
            OptionalNumeric::Null => write!(f, "null"),
        }
    }
}

/// 可选浮点数类型，用于期货特有字段（持仓量、结算价等）
pub type OptionalF64 = OptionalNumeric<f64>;

/// 可选整数类型
pub type OptionalI64 = OptionalNumeric<i64>;

/// 标准化市场行情快照（L1/L2深度）
///
/// 支持沪深股票、期货、期权、ETF等多种品种。
/// - 必填字段：L1买卖价量、最新价、成交量等
/// - 可选字段：L2深度（ask/bid price/volume 2~10）
/// - 特殊字段：期货持仓量、ETF的IOPV，用 `OptionalF64` 表示
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MDSnapshot {
    /// 合约唯一标识，如 "SSE.688286" / "SHFE.rb2501"
    pub instrument_id: String,

    /// 成交额（元）
    pub amount: f64,

    /// 卖一价
    pub ask_price1: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price3: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price4: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price5: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price6: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price7: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price8: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price9: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price10: Option<f64>,

    /// 卖一量
    pub ask_volume1: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume2: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume3: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume4: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume5: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume6: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume7: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume8: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume9: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume10: Option<i64>,

    /// 买一价
    pub bid_price1: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price3: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price4: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price5: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price6: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price7: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price8: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price9: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price10: Option<f64>,

    /// 买一量
    pub bid_volume1: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume2: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume3: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume4: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume5: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume6: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume7: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume8: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume9: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume10: Option<i64>,

    /// 收盘价（收盘前为"-"）
    pub close: OptionalF64,

    /// 快照时间戳（UTC）
    pub datetime: DateTime<Utc>,

    /// 当日最高价
    pub highest: f64,

    /// 最新成交价
    pub last_price: f64,

    /// 跌停价
    pub lower_limit: f64,

    /// 当日最低价
    pub lowest: f64,

    /// 开盘价
    pub open: f64,

    /// 持仓量（期货/期权特有，股票为"-"）
    pub open_interest: OptionalF64,

    /// 昨收盘价
    pub pre_close: f64,

    /// 昨持仓量
    pub pre_open_interest: OptionalF64,

    /// 昨结算价（期货特有）
    pub pre_settlement: OptionalF64,

    /// 结算价（期货特有，收盘前为"-"）
    pub settlement: OptionalF64,

    /// 涨停价
    pub upper_limit: f64,

    /// 当日成交量（手）
    pub volume: i64,

    /// 成交均价（VWAP）
    pub average: f64,

    /// ETF参考净值（IOPV），非ETF为Null
    pub iopv: OptionalF64,
}

impl MDSnapshot {
    /// 是否包含L2深度数据
    pub fn has_level2_depth(&self) -> bool {
        self.ask_price2.is_some() || self.bid_price2.is_some()
    }

    /// 是否为期货或期权品种（有持仓量）
    pub fn is_futures_or_options(&self) -> bool {
        matches!(self.open_interest, OptionalF64::Value(_))
    }

    /// 是否为ETF品种（有IOPV）
    pub fn is_etf(&self) -> bool {
        matches!(self.iopv, OptionalF64::Value(_))
    }

    /// 买卖价差
    pub fn bid_ask_spread(&self) -> f64 {
        self.ask_price1 - self.bid_price1
    }

    /// 提取简化Tick
    pub fn to_tick(&self) -> Tick {
        Tick::from_snapshot(self)
    }
}

/// 简化Tick数据，仅含最新价和成交量
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
        Self {
            instrument_id,
            last_price,
            volume,
            amount,
            datetime,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bid_ask_spread() {
        let snap = MDSnapshot {
            instrument_id: "SSE.688286".to_string(),
            amount: 1_000_000.0,
            ask_price1: 10.5,
            ask_volume1: 100,
            bid_price1: 10.4,
            bid_volume1: 150,
            last_price: 10.45,
            datetime: Utc::now(),
            highest: 10.6,
            lowest: 10.3,
            open: 10.35,
            close: OptionalF64::Value(10.5),
            volume: 25_000,
            pre_close: 10.3,
            lower_limit: 9.3,
            upper_limit: 11.3,
            average: 10.45,
            open_interest: OptionalF64::String("-".to_string()),
            pre_open_interest: OptionalF64::String("-".to_string()),
            pre_settlement: OptionalF64::String("-".to_string()),
            settlement: OptionalF64::String("-".to_string()),
            iopv: OptionalF64::Null,
            ask_price2: None,
            ask_price3: None,
            ask_price4: None,
            ask_price5: None,
            ask_price6: None,
            ask_price7: None,
            ask_price8: None,
            ask_price9: None,
            ask_price10: None,
            ask_volume2: None,
            ask_volume3: None,
            ask_volume4: None,
            ask_volume5: None,
            ask_volume6: None,
            ask_volume7: None,
            ask_volume8: None,
            ask_volume9: None,
            ask_volume10: None,
            bid_price2: None,
            bid_price3: None,
            bid_price4: None,
            bid_price5: None,
            bid_price6: None,
            bid_price7: None,
            bid_price8: None,
            bid_price9: None,
            bid_price10: None,
            bid_volume2: None,
            bid_volume3: None,
            bid_volume4: None,
            bid_volume5: None,
            bid_volume6: None,
            bid_volume7: None,
            bid_volume8: None,
            bid_volume9: None,
            bid_volume10: None,
        };
        let spread = snap.bid_ask_spread();
        assert!((spread - 0.1).abs() < 1e-9);
        assert!(!snap.has_level2_depth());
        assert!(!snap.is_futures_or_options());
        assert!(!snap.is_etf());
    }

    #[test]
    fn test_tick_from_snapshot() {
        let now = Utc::now();
        let snap = MDSnapshot {
            instrument_id: "SHFE.rb2501".to_string(),
            amount: 5_000_000.0,
            ask_price1: 3500.0,
            ask_volume1: 10,
            bid_price1: 3498.0,
            bid_volume1: 20,
            last_price: 3499.0,
            datetime: now,
            highest: 3520.0,
            lowest: 3480.0,
            open: 3490.0,
            close: OptionalF64::String("-".to_string()),
            volume: 12_000,
            pre_close: 3485.0,
            lower_limit: 3360.0,
            upper_limit: 3610.0,
            average: 3499.5,
            open_interest: OptionalF64::Value(150_000.0),
            pre_open_interest: OptionalF64::Value(148_000.0),
            pre_settlement: OptionalF64::Value(3485.0),
            settlement: OptionalF64::String("-".to_string()),
            iopv: OptionalF64::Null,
            ask_price2: None,
            ask_price3: None,
            ask_price4: None,
            ask_price5: None,
            ask_price6: None,
            ask_price7: None,
            ask_price8: None,
            ask_price9: None,
            ask_price10: None,
            ask_volume2: None,
            ask_volume3: None,
            ask_volume4: None,
            ask_volume5: None,
            ask_volume6: None,
            ask_volume7: None,
            ask_volume8: None,
            ask_volume9: None,
            ask_volume10: None,
            bid_price2: None,
            bid_price3: None,
            bid_price4: None,
            bid_price5: None,
            bid_price6: None,
            bid_price7: None,
            bid_price8: None,
            bid_price9: None,
            bid_price10: None,
            bid_volume2: None,
            bid_volume3: None,
            bid_volume4: None,
            bid_volume5: None,
            bid_volume6: None,
            bid_volume7: None,
            bid_volume8: None,
            bid_volume9: None,
            bid_volume10: None,
        };
        assert!(snap.is_futures_or_options());
        let tick = snap.to_tick();
        assert_eq!(tick.instrument_id, "SHFE.rb2501");
        assert_eq!(tick.last_price, 3499.0);
        assert_eq!(tick.datetime, now);
    }

    fn make_minimal_snap(instrument_id: &str) -> MDSnapshot {
        use chrono::TimeZone;
        MDSnapshot {
            instrument_id: instrument_id.to_string(),
            amount: 0.0,
            ask_price1: 10.0,
            ask_volume1: 100,
            bid_price1: 9.9,
            bid_volume1: 100,
            last_price: 9.95,
            datetime: Utc.with_ymd_and_hms(2026, 4, 20, 9, 30, 0).unwrap(),
            highest: 10.1,
            lowest: 9.8,
            open: 9.9,
            close: OptionalF64::String("-".to_string()),
            volume: 1000,
            pre_close: 9.85,
            lower_limit: 8.9,
            upper_limit: 10.8,
            average: 9.95,
            open_interest: OptionalF64::Null,
            pre_open_interest: OptionalF64::Null,
            pre_settlement: OptionalF64::Null,
            settlement: OptionalF64::Null,
            iopv: OptionalF64::Null,
            ask_price2: None, ask_price3: None, ask_price4: None, ask_price5: None,
            ask_price6: None, ask_price7: None, ask_price8: None, ask_price9: None, ask_price10: None,
            ask_volume2: None, ask_volume3: None, ask_volume4: None, ask_volume5: None,
            ask_volume6: None, ask_volume7: None, ask_volume8: None, ask_volume9: None, ask_volume10: None,
            bid_price2: None, bid_price3: None, bid_price4: None, bid_price5: None,
            bid_price6: None, bid_price7: None, bid_price8: None, bid_price9: None, bid_price10: None,
            bid_volume2: None, bid_volume3: None, bid_volume4: None, bid_volume5: None,
            bid_volume6: None, bid_volume7: None, bid_volume8: None, bid_volume9: None, bid_volume10: None,
        }
    }

    #[test]
    fn test_json_roundtrip_l1_only() {
        let snap = make_minimal_snap("SSE.688286");
        let json = serde_json::to_string(&snap).expect("serialize");
        let snap2: MDSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(snap, snap2);
    }

    #[test]
    fn test_json_roundtrip_null_optional_fields() {
        let snap = make_minimal_snap("SSE.000001");
        // All optional numeric fields are Null
        assert_eq!(snap.open_interest, OptionalF64::Null);
        assert_eq!(snap.iopv, OptionalF64::Null);
        let json = serde_json::to_string(&snap).expect("serialize");
        let snap2: MDSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(snap2.open_interest, OptionalF64::Null);
        assert_eq!(snap2.iopv, OptionalF64::Null);
    }

    #[test]
    fn test_json_roundtrip_dash_string_optional() {
        // close = "-" (盘中收盘价未确定), settlement = "-"
        let snap = make_minimal_snap("SHFE.ag2606");
        assert!(matches!(snap.close, OptionalF64::String(_)));
        let json = serde_json::to_string(&snap).expect("serialize");
        let snap2: MDSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(snap2.close, OptionalF64::String(_)));
    }

    #[test]
    fn test_json_roundtrip_futures_optional_value() {
        use chrono::TimeZone;
        let mut snap = make_minimal_snap("SHFE.rb2601");
        snap.open_interest = OptionalF64::Value(150_000.0);
        snap.pre_open_interest = OptionalF64::Value(148_000.0);
        snap.pre_settlement = OptionalF64::Value(3485.0);
        let json = serde_json::to_string(&snap).expect("serialize");
        let snap2: MDSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(snap2.open_interest, OptionalF64::Value(150_000.0));
        assert_eq!(snap2.pre_settlement, OptionalF64::Value(3485.0));
        assert!(snap2.is_futures_or_options());
    }

    #[test]
    fn test_json_roundtrip_l2_full_depth() {
        use chrono::TimeZone;
        let mut snap = make_minimal_snap("SSE.600000");
        // 设置完整 10 档盘口
        snap.ask_price2 = Some(10.01); snap.ask_price3 = Some(10.02);
        snap.ask_price4 = Some(10.03); snap.ask_price5 = Some(10.04);
        snap.ask_price6 = Some(10.05); snap.ask_price7 = Some(10.06);
        snap.ask_price8 = Some(10.07); snap.ask_price9 = Some(10.08);
        snap.ask_price10 = Some(10.09);
        snap.bid_price2 = Some(9.89); snap.bid_price3 = Some(9.88);
        snap.bid_price4 = Some(9.87); snap.bid_price5 = Some(9.86);
        snap.bid_price6 = Some(9.85); snap.bid_price7 = Some(9.84);
        snap.bid_price8 = Some(9.83); snap.bid_price9 = Some(9.82);
        snap.bid_price10 = Some(9.81);
        snap.ask_volume2 = Some(200); snap.bid_volume2 = Some(300);
        assert!(snap.has_level2_depth());
        let json = serde_json::to_string(&snap).expect("serialize");
        let snap2: MDSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(snap, snap2);
        assert!(snap2.has_level2_depth());
        assert_eq!(snap2.ask_price10, Some(10.09));
        assert_eq!(snap2.bid_price10, Some(9.81));
    }

    #[test]
    fn test_json_roundtrip_etf_iopv() {
        let mut snap = make_minimal_snap("SSE.510300");
        snap.iopv = OptionalF64::Value(4.523);
        assert!(snap.is_etf());
        let json = serde_json::to_string(&snap).expect("serialize");
        let snap2: MDSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(snap2.iopv, OptionalF64::Value(4.523));
        assert!(snap2.is_etf());
    }

    #[test]
    fn test_l1_json_does_not_contain_l2_fields() {
        let snap = make_minimal_snap("SSE.688286");
        let json = serde_json::to_string(&snap).expect("serialize");
        // skip_serializing_if = "Option::is_none" → L2 fields absent from JSON
        assert!(!json.contains("ask_price2"));
        assert!(!json.contains("bid_price2"));
    }
}
