#![allow(dead_code)]
//! MongoDB 历史数据回放骨架。
//!
//! 第一阶段目标不是一次性覆盖所有集合，而是先提供稳定的回放接口，
//! 让 LiveEngine / SignalGenerator 能与实时行情共享同一消费方式。

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::sync::Client;

use crate::qadatastruct::mdsnapshot::{MDSnapshot, OptionalF64};
use crate::qamarket::live_types::{MarketDataEnvelope, MarketDataPullSource, MarketDataSource, SourceHealth};
use crate::qamarket::qamdgateway::MarketDataSource as GatewayMarketDataSource;
use crate::qaconnector::mongo::stock_day::{order_book_id_from_mongo_code, qa_date_stamp_yyyy_mm_dd};

#[derive(Debug, Clone)]
pub struct ReplayConfig {
    pub source_name: String,
    pub market_type: GatewayMarketDataSource,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            source_name: "MongoReplay".to_string(),
            market_type: GatewayMarketDataSource::Custom,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MongoReplayCollectionConfig {
    pub uri: String,
    pub database: String,
    pub collection: String,
    pub market_type: GatewayMarketDataSource,
}

impl MongoReplayCollectionConfig {
    pub fn new(
        uri: String,
        database: String,
        collection: String,
        market_type: GatewayMarketDataSource,
    ) -> Self {
        Self {
            uri,
            database,
            collection,
            market_type,
        }
    }
}

/// MongoDB 回放事件
#[derive(Debug, Clone)]
pub struct ReplayEvent {
    pub instrument_id: String,
    pub datetime: DateTime<Utc>,
    pub price: f64,
    pub volume: i64,
    pub amount: f64,
}

impl ReplayEvent {
    pub fn into_snapshot(self, market_type: GatewayMarketDataSource) -> MarketDataEnvelope {
        MarketDataEnvelope {
            source: market_type,
            replay: true,
            snapshot: MDSnapshot {
                instrument_id: self.instrument_id,
                amount: self.amount,
                ask_price1: self.price,
                ask_price2: None,
                ask_price3: None,
                ask_price4: None,
                ask_price5: None,
                ask_price6: None,
                ask_price7: None,
                ask_price8: None,
                ask_price9: None,
                ask_price10: None,
                ask_volume1: self.volume,
                ask_volume2: None,
                ask_volume3: None,
                ask_volume4: None,
                ask_volume5: None,
                ask_volume6: None,
                ask_volume7: None,
                ask_volume8: None,
                ask_volume9: None,
                ask_volume10: None,
                bid_price1: self.price,
                bid_price2: None,
                bid_price3: None,
                bid_price4: None,
                bid_price5: None,
                bid_price6: None,
                bid_price7: None,
                bid_price8: None,
                bid_price9: None,
                bid_price10: None,
                bid_volume1: self.volume,
                bid_volume2: None,
                bid_volume3: None,
                bid_volume4: None,
                bid_volume5: None,
                bid_volume6: None,
                bid_volume7: None,
                bid_volume8: None,
                bid_volume9: None,
                bid_volume10: None,
                close: OptionalF64::Null,
                datetime: self.datetime,
                highest: self.price,
                last_price: self.price,
                lower_limit: self.price,
                lowest: self.price,
                open: self.price,
                open_interest: OptionalF64::Null,
                pre_close: self.price,
                pre_open_interest: OptionalF64::Null,
                pre_settlement: OptionalF64::Null,
                settlement: OptionalF64::Null,
                upper_limit: self.price,
                volume: self.volume,
                average: self.price,
                iopv: OptionalF64::Null,
            },
        }
    }
}

/// 最小可用回放源。
///
/// 当前仅负责承载统一接口，后续可接 stock_day / future_day / tick 集合。
pub struct MongoReplaySource {
    pub config: ReplayConfig,
    pub subscriptions: Vec<String>,
    pub cursor: usize,
    pub events: Vec<ReplayEvent>,
}

impl MongoReplaySource {
    pub fn new(config: ReplayConfig, events: Vec<ReplayEvent>) -> Self {
        Self {
            config,
            subscriptions: Vec::new(),
            cursor: 0,
            events,
        }
    }

    pub fn from_events(
        source_name: &str,
        market_type: GatewayMarketDataSource,
        mut events: Vec<ReplayEvent>,
    ) -> Self {
        events.sort_by_key(|item| item.datetime);
        Self::new(
            ReplayConfig {
                source_name: source_name.to_string(),
                market_type,
            },
            events,
        )
    }
}

impl MarketDataSource for MongoReplaySource {
    fn name(&self) -> &str {
        &self.config.source_name
    }

    fn source_type(&self) -> GatewayMarketDataSource {
        self.config.market_type
    }

    fn health_check(&self) -> SourceHealth {
        SourceHealth::Healthy
    }

    fn subscribe(&mut self, instruments: &[String]) -> Result<(), String> {
        self.subscriptions = instruments.to_vec();
        Ok(())
    }

    fn unsubscribe(&mut self, instruments: &[String]) -> Result<(), String> {
        self.subscriptions
            .retain(|code| !instruments.iter().any(|item| item == code));
        Ok(())
    }
}

impl MarketDataPullSource for MongoReplaySource {
    fn next_event(&mut self) -> Result<Option<MarketDataEnvelope>, String> {
        while self.cursor < self.events.len() {
            let event = self.events[self.cursor].clone();
            self.cursor += 1;
            if self.subscriptions.is_empty()
                || self.subscriptions.iter().any(|item| item == &event.instrument_id)
            {
                return Ok(Some(event.into_snapshot(self.config.market_type)));
            }
        }
        Ok(None)
    }
}

fn bson_f64(doc: &Document, key: &str) -> Option<f64> {
    match doc.get(key) {
        Some(Bson::Double(v)) => Some(*v),
        Some(Bson::Int32(v)) => Some(*v as f64),
        Some(Bson::Int64(v)) => Some(*v as f64),
        Some(Bson::String(v)) => v.parse::<f64>().ok(),
        _ => None,
    }
}

fn bson_i64(doc: &Document, key: &str) -> Option<i64> {
    match doc.get(key) {
        Some(Bson::Int32(v)) => Some(*v as i64),
        Some(Bson::Int64(v)) => Some(*v),
        Some(Bson::Double(v)) => Some(*v as i64),
        Some(Bson::String(v)) => v.parse::<i64>().ok(),
        _ => None,
    }
}

fn bson_string(doc: &Document, key: &str) -> Option<String> {
    match doc.get(key) {
        Some(Bson::String(v)) => Some(v.clone()),
        Some(Bson::Int32(v)) => Some(v.to_string()),
        Some(Bson::Int64(v)) => Some(v.to_string()),
        _ => None,
    }
}

fn local_naive_to_utc(naive: NaiveDateTime) -> Result<DateTime<Utc>, String> {
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| "ambiguous local datetime".to_string())
}

fn parse_datetime_like(text: &str) -> Result<DateTime<Utc>, String> {
    let cleaned = text.trim();
    let candidates = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
    ];
    for pattern in candidates {
        if let Ok(naive) = NaiveDateTime::parse_from_str(cleaned, pattern) {
            return local_naive_to_utc(naive);
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(cleaned, "%Y-%m-%d") {
        return local_naive_to_utc(
            date.and_hms_opt(15, 0, 0)
                .ok_or_else(|| "invalid date".to_string())?,
        );
    }
    Err(format!("unsupported datetime format: {}", cleaned))
}

fn stock_day_doc_to_event(doc: &Document) -> Result<Option<ReplayEvent>, String> {
    let code = match bson_string(doc, "code") {
        Some(code) => order_book_id_from_mongo_code(&code),
        None => return Ok(None),
        };
    let price = bson_f64(doc, "close").unwrap_or(0.0);
    let amount = bson_f64(doc, "amount").unwrap_or(0.0);
    let volume = bson_f64(doc, "vol")
        .or_else(|| bson_f64(doc, "volume"))
        .unwrap_or(0.0) as i64;
    let date = match bson_string(doc, "date") {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(Some(ReplayEvent {
        instrument_id: code,
        datetime: parse_datetime_like(&date)?,
        price,
        volume,
        amount,
    }))
}

fn stock_min_doc_to_event(doc: &Document) -> Result<Option<ReplayEvent>, String> {
    let code = match bson_string(doc, "code") {
        Some(code) => order_book_id_from_mongo_code(&code),
        None => return Ok(None),
    };
    let datetime = match bson_string(doc, "datetime") {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(Some(ReplayEvent {
        instrument_id: code,
        datetime: parse_datetime_like(&datetime)?,
        price: bson_f64(doc, "close").unwrap_or(0.0),
        volume: bson_f64(doc, "vol")
            .or_else(|| bson_f64(doc, "volume"))
            .unwrap_or(0.0) as i64,
        amount: bson_f64(doc, "amount").unwrap_or(0.0),
    }))
}

fn future_day_doc_to_event(doc: &Document) -> Result<Option<ReplayEvent>, String> {
    let code = match bson_string(doc, "code") {
        Some(code) => code,
        None => return Ok(None),
    };
    let date = match bson_string(doc, "date") {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(Some(ReplayEvent {
        instrument_id: code,
        datetime: parse_datetime_like(&date)?,
        price: bson_f64(doc, "close").unwrap_or(0.0),
        volume: bson_f64(doc, "position")
            .or_else(|| bson_f64(doc, "vol"))
            .or_else(|| bson_f64(doc, "volume"))
            .unwrap_or(0.0) as i64,
        amount: bson_f64(doc, "amount").unwrap_or(0.0),
    }))
}

fn future_min_doc_to_event(doc: &Document) -> Result<Option<ReplayEvent>, String> {
    let code = match bson_string(doc, "code") {
        Some(code) => code,
        None => return Ok(None),
    };
    let datetime = match bson_string(doc, "datetime") {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(Some(ReplayEvent {
        instrument_id: code,
        datetime: parse_datetime_like(&datetime)?,
        price: bson_f64(doc, "close").unwrap_or(0.0),
        volume: bson_f64(doc, "position")
            .or_else(|| bson_f64(doc, "vol"))
            .or_else(|| bson_f64(doc, "volume"))
            .unwrap_or(0.0) as i64,
        amount: bson_f64(doc, "amount").unwrap_or(0.0),
    }))
}

fn collection_doc_to_event(collection: &str, doc: &Document) -> Result<Option<ReplayEvent>, String> {
    match collection {
        "stock_day" => stock_day_doc_to_event(doc),
        "stock_min" => stock_min_doc_to_event(doc),
        "future_day" => future_day_doc_to_event(doc),
        "future_min" => future_min_doc_to_event(doc),
        other => Err(format!("unsupported replay collection: {}", other)),
    }
}

fn normalize_code_query(code: &str, collection: &str) -> String {
    match collection {
        "stock_day" | "stock_min" => code
            .chars()
            .filter(|ch| ch.is_ascii_digit())
            .take(6)
            .collect(),
        _ => code.to_string(),
    }
}

fn date_filter_for_collection(
    collection: &str,
    start: &str,
    end: &str,
) -> Result<Document, String> {
    match collection {
        "stock_day" => Ok(doc! {
            "date_stamp": {
                "$gte": qa_date_stamp_yyyy_mm_dd(start)?,
                "$lte": qa_date_stamp_yyyy_mm_dd(end)?,
            }
        }),
        "stock_min" | "future_min" => Ok(doc! {
            "datetime": {
                "$gte": format!("{} 00:00:00", &start[..10.min(start.len())]),
                "$lte": format!("{} 23:59:59", &end[..10.min(end.len())]),
            }
        }),
        "future_day" => Ok(doc! {
            "date": {
                "$gte": &start[..10.min(start.len())],
                "$lte": &end[..10.min(end.len())],
            }
        }),
        other => Err(format!("unsupported replay collection: {}", other)),
    }
}

pub fn load_replay_events(
    cfg: &MongoReplayCollectionConfig,
    codes: &[String],
    start: &str,
    end: &str,
) -> Result<Vec<ReplayEvent>, String> {
    let client = Client::with_uri_str(&cfg.uri).map_err(|e| e.to_string())?;
    let coll = client
        .database(&cfg.database)
        .collection(&cfg.collection);

    let normalized_codes: Vec<String> = codes
        .iter()
        .map(|code| normalize_code_query(code, &cfg.collection))
        .collect();
    let mut filter = date_filter_for_collection(&cfg.collection, start, end)?;
    filter.insert("code", doc! { "$in": normalized_codes });

    let opts = FindOptions::builder()
        .projection(doc! { "_id": 0 })
        .batch_size(10_000u32)
        .build();
    let cursor = coll.find(filter, opts).map_err(|e| e.to_string())?;

    let mut events = Vec::new();
    for res in cursor {
        let doc = res.map_err(|e| format!("cursor: {}", e))?;
        if let Some(event) = collection_doc_to_event(&cfg.collection, &doc)? {
            events.push(event);
        }
    }
    events.sort_by_key(|item| item.datetime);
    Ok(events)
}

pub fn load_replay_source(
    cfg: &MongoReplayCollectionConfig,
    source_name: &str,
    codes: &[String],
    start: &str,
    end: &str,
) -> Result<MongoReplaySource, String> {
    let events = load_replay_events(cfg, codes, start, end)?;
    Ok(MongoReplaySource::from_events(
        source_name,
        cfg.market_type,
        events,
    ))
}
