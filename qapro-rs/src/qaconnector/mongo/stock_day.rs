//! 从 MongoDB 读取 QUANTAXIS `stock_day` 集合，供 Rust 回测 / Polars 使用。
//! 查询语义与 Python `QA_fetch_stock_day` 一致：`code` + `date_stamp` 区间。

use std::collections::HashSet;

use chrono::{Local, NaiveDate, TimeZone};
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::sync::Client;

use crate::qadatastruct::stockday::QADataStruct_StockDay;

/// 与 Python `QA_util_date_stamp`（`time.mktime` 本地时区）对齐：某日 00:00 **本机本地时区** 的 Unix 秒。
pub fn qa_date_stamp_yyyy_mm_dd(date: &str) -> Result<f64, String> {
    let d = date.trim();
    let d10 = if d.len() >= 10 { &d[..10] } else { d };
    let naive = NaiveDate::parse_from_str(d10, "%Y-%m-%d")
        .map_err(|e| format!("parse date {}: {}", d10, e))?;
    let ndt = naive
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "invalid time".to_string())?;
    let dt = Local
        .from_local_datetime(&ndt)
        .single()
        .ok_or_else(|| "ambiguous local time".to_string())?;
    Ok(dt.timestamp() as f64)
}

/// 将 Mongo 中的 6 位 `code` 转为与 ClickHouse 示例一致的 `order_book_id`（.XSHG / .XSHE）。
pub fn order_book_id_from_mongo_code(code: &str) -> String {
    let s = code.trim();
    if s.contains('.') && s.len() > 7 {
        return s.to_string();
    }
    let six: String = s.chars().filter(|c| c.is_ascii_digit()).take(6).collect();
    if six.len() < 6 {
        return format!("{}.XSHE", s);
    }
    let ob = match six.chars().next() {
        Some('6') | Some('9') => format!("{}.XSHG", six),
        _ => format!("{}.XSHE", six),
    };
    ob
}

fn normalize_mongo_code(code: &str) -> String {
    let s = code.trim();
    if s.contains('.') {
        s.chars().filter(|c| c.is_ascii_digit()).take(6).collect()
    } else {
        s.chars().filter(|c| c.is_ascii_digit()).take(6).collect()
    }
}

fn bson_f32(doc: &Document, key: &str) -> f32 {
    match doc.get(key) {
        Some(Bson::Double(d)) => *d as f32,
        Some(Bson::Int32(i)) => *i as f32,
        Some(Bson::Int64(i)) => *i as f32,
        _ => f32::NAN,
    }
}

fn bson_date_string(doc: &Document) -> Option<String> {
    match doc.get("date")? {
        Bson::String(s) => {
            let t = s.trim();
            Some(if t.len() >= 10 {
                t[..10].to_string()
            } else {
                t.to_string()
            })
        }
        Bson::DateTime(dt) => Some(dt.format("%Y-%m-%d").to_string()),
        _ => None,
    }
}

/// Mongo 连接参数（与 `example.toml` 中 `[hisdata]` 对应）。
#[derive(Clone, Debug)]
pub struct MongoStockDayConfig {
    pub uri: String,
    pub database: String,
    pub collection: String,
}

impl Default for MongoStockDayConfig {
    fn default() -> Self {
        Self {
            uri: "mongodb://127.0.0.1:27017".to_string(),
            database: "quantaxis".to_string(),
            collection: "stock_day".to_string(),
        }
    }
}

/// 从 `stock_list` 读取全部 6 位 `code`（去重），用于批量拉日线。
pub fn fetch_stock_codes_from_list(cfg: &MongoStockDayConfig) -> Result<Vec<String>, String> {
    let client = Client::with_uri_str(&cfg.uri).map_err(|e| e.to_string())?;
    let coll = client.database(&cfg.database).collection("stock_list");
    let cursor = coll
        .find(None, None)
        .map_err(|e| format!("stock_list find: {}", e))?;
    let mut set: HashSet<String> = HashSet::new();
    for res in cursor {
        let doc = res.map_err(|e| format!("stock_list cursor: {}", e))?;
        let n = match doc.get("code") {
            Some(Bson::String(c)) => normalize_mongo_code(c),
            Some(Bson::Int32(i)) => format!("{:06}", *i),
            Some(Bson::Int64(i)) => format!("{:06}", *i),
            _ => continue,
        };
        if n.len() == 6 {
            set.insert(n);
        }
    }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    Ok(v)
}

/// 按 Python `QA_fetch_stock_day` 条件查询 Mongo，并构建 `QADataStruct_StockDay`。
///
/// - `codes`：6 位代码或带后缀均可，内部统一为 6 位参与 `$in`。
/// - `start` / `end`：`YYYY-MM-DD`。
/// - 跳过 `vol <= 1` 的行（与 Python `.query('volume>1')` 一致）。
pub fn load_stock_day_for_backtest(
    cfg: &MongoStockDayConfig,
    codes: &[String],
    start: &str,
    end: &str,
) -> Result<QADataStruct_StockDay, String> {
    if codes.is_empty() {
        return Err("codes is empty".to_string());
    }
    let start_stamp = qa_date_stamp_yyyy_mm_dd(start)?;
    let end_stamp = qa_date_stamp_yyyy_mm_dd(end)?;
    if start_stamp > end_stamp {
        return Err("start > end".to_string());
    }

    let codes_6: Vec<String> = codes.iter().map(|c| normalize_mongo_code(c)).collect();

    let client = Client::with_uri_str(&cfg.uri).map_err(|e| e.to_string())?;
    let coll = client.database(&cfg.database).collection(&cfg.collection);

    let filter = doc! {
        "code": { "$in": codes_6 },
        "date_stamp": { "$gte": start_stamp, "$lte": end_stamp }
    };
    let opts = FindOptions::builder()
        .projection(doc! {"_id": 0})
        .batch_size(10_000u32)
        .build();

    let cursor = coll
        .find(filter, opts)
        .map_err(|e| format!("stock_day find: {}", e))?;

    let mut dates: Vec<String> = Vec::new();
    let mut obid: Vec<String> = Vec::new();
    let mut open: Vec<f32> = Vec::new();
    let mut high: Vec<f32> = Vec::new();
    let mut low: Vec<f32> = Vec::new();
    let mut close: Vec<f32> = Vec::new();
    let mut limit_up: Vec<f32> = Vec::new();
    let mut limit_down: Vec<f32> = Vec::new();
    let mut num_trades: Vec<f32> = Vec::new();
    let mut volume: Vec<f32> = Vec::new();
    let mut total_turnover: Vec<f32> = Vec::new();
    let mut amount: Vec<f32> = Vec::new();

    for res in cursor {
        let doc = res.map_err(|e| format!("cursor: {}", e))?;
        let vol = bson_f32(&doc, "vol");
        if !vol.is_nan() && vol <= 1.0 {
            continue;
        }
        let date_s = match bson_date_string(&doc) {
            Some(d) => d,
            None => continue,
        };
        let code_raw = match doc.get("code") {
            Some(Bson::String(s)) => s.clone(),
            _ => continue,
        };
        let c6 = normalize_mongo_code(&code_raw);
        if c6.len() != 6 {
            continue;
        }
        let o = bson_f32(&doc, "open");
        let h = bson_f32(&doc, "high");
        let l = bson_f32(&doc, "low");
        let c = bson_f32(&doc, "close");
        if o.is_nan() || h.is_nan() || l.is_nan() || c.is_nan() {
            continue;
        }
        if o == 0.0 && h == 0.0 && l == 0.0 && c == 0.0 {
            continue;
        }

        let am = bson_f32(&doc, "amount");
        let lu = bson_f32(&doc, "limit_up");
        let ld = bson_f32(&doc, "limit_down");
        let nt = bson_f32(&doc, "num_trades");

        dates.push(date_s);
        obid.push(order_book_id_from_mongo_code(&c6));
        open.push(o);
        high.push(h);
        low.push(l);
        close.push(c);
        limit_up.push(if lu.is_nan() { 0.0 } else { lu });
        limit_down.push(if ld.is_nan() { 0.0 } else { ld });
        num_trades.push(if nt.is_nan() { 0.0 } else { nt });
        volume.push(if vol.is_nan() { 0.0 } else { vol });
        let amt = if !am.is_nan() { am } else { 0.0 };
        total_turnover.push(amt);
        amount.push(amt);
    }

    if dates.is_empty() {
        return Err("no rows returned (check codes, date range, and Mongo data)".to_string());
    }

    Ok(QADataStruct_StockDay::new_from_vec(
        dates,
        obid,
        open,
        high,
        low,
        close,
        limit_up,
        limit_down,
        num_trades,
        volume,
        total_turnover,
    ))
}

/// 使用 `example.toml` 里 `[hisdata]` 的 uri / db，集合固定为 `stock_day`。
/// 需已通过命令行参数加载过 `CONFIG`（与 `main`/示例一致）。
pub fn load_stock_day_from_app_config(
    codes: &[String],
    start: &str,
    end: &str,
) -> Result<QADataStruct_StockDay, String> {
    use crate::qaenv::localenv::CONFIG;
    let cfg = MongoStockDayConfig {
        uri: CONFIG.hisdata.uri.clone(),
        database: CONFIG.hisdata.db.clone(),
        collection: "stock_day".to_string(),
    };
    load_stock_day_for_backtest(&cfg, codes, start, end)
}
