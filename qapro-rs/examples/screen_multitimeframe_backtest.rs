//! 多周期技术形态筛选（基于全市场日 K 缓存）
//!
//! 条件（在各自周期「最新一根」上判定，与最新交易日对齐）：
//! - **月线**：最近 5 个自然月收盘，连续 4 段上涨，即 `m0 > m1 > m2 > m3 > m4`
//!   （等价于连续 4 个月月线收阳/创新高台阶）。
//! - **周线**：最近一周收盘 **低于** 前一周；且前一周、再前一周、再再前一周
//!   各 **高于** 其前一周，即「先连涨 3 周，最近 1 周回调」：
//!   `w0 < w1` 且 `w1 > w2 > w3 > w4`。
//! - **250 日新高**：最新交易日最高价与过去 250 个交易日（含当日）最高价 rolling max
//!   一致（允许 0.01 元浮点误差）。
//!
//! 周线/月线：按 **ISO 周**（`%G-%V`）与 **日历月**（`%Y-%m`）聚合，
//! 周内/月内取 **按交易日排序后最后一个交易日** 的收盘价作为该周/月收盘。
//!
//! 数据：与 `backtest.rs` 相同，默认读取 `CONFIG.DataPath.cache` 下 `stockdayqfq.parquet`。

use std::time::Instant;

use polars::prelude::*;

use qapro_rs::qadatastruct::stockday::QADataStruct_StockDay;
use qapro_rs::qaenv::localenv::CONFIG;

fn main() {
    let cache_file = format!("{}stockdayqfq.parquet", &CONFIG.DataPath.cache);
    let sw = Instant::now();
    let qfq = QADataStruct_StockDay::new_from_parquet(cache_file.as_str());
    println!("loaded parquet in {:?}", sw.elapsed());

    let lf = qfq
        .data
        .lazy()
        .with_columns([col("date")
            .str()
            .to_date(StrptimeOptions {
                format: Some("%Y-%m-%d".into()),
                ..Default::default()
            })
            .alias("d")])
        .sort(
            ["order_book_id", "d"],
            SortMultipleOptions::default().with_order_descending_multi([false, false]),
        );

    let roll_opts = RollingOptionsFixedWindow {
        window_size: 250,
        min_periods: 250,
        weights: None,
        center: false,
        fn_params: None,
    };

    // 最新交易日 + 250 日最高价（用最高价触及区间高点）
    let daily_hit = lf
        .clone()
        .with_columns([
            col("high")
                .rolling_max(roll_opts)
                .over([col("order_book_id")])
                .alias("high250max"),
            col("d").max().over([col("order_book_id")]).alias("last_d"),
        ])
        .filter(col("d").eq(col("last_d")))
        .filter(
            col("high")
                .gt_eq(col("high250max") - lit(0.01f64))
                .and(col("high").lt_eq(col("high250max") + lit(0.01f64))),
        )
        .select([
            col("order_book_id"),
            col("date"),
            col("close"),
            col("high"),
        ]);

    // 周线：ISO 周聚合后，最新一周弱于前一周，且前连续三周每周收涨
    let weekly_keys = lf
        .clone()
        .with_columns([col("d").dt().strftime("%G-%V").alias("iso_yw")]);
    let weekly_hit = weekly_keys
        .sort(
            ["order_book_id", "iso_yw", "d"],
            SortMultipleOptions::default().with_order_descending_multi([false, false, false]),
        )
        .group_by([col("order_book_id"), col("iso_yw")])
        .agg([
            col("close").last().alias("w_close"),
            col("d").last().alias("w_end"),
        ])
        .sort(
            ["order_book_id", "w_end"],
            SortMultipleOptions::default().with_order_descending_multi([false, false]),
        )
        .with_columns([
            col("w_close")
                .shift(lit(1i64))
                .over([col("order_book_id")])
                .alias("w1"),
            col("w_close")
                .shift(lit(2i64))
                .over([col("order_book_id")])
                .alias("w2"),
            col("w_close")
                .shift(lit(3i64))
                .over([col("order_book_id")])
                .alias("w3"),
            col("w_close")
                .shift(lit(4i64))
                .over([col("order_book_id")])
                .alias("w4"),
            col("w_end")
                .max()
                .over([col("order_book_id")])
                .alias("last_w_end"),
        ])
        .filter(col("w_end").eq(col("last_w_end")))
        .filter(
            col("w_close")
                .lt(col("w1"))
                .and(col("w1").gt(col("w2")))
                .and(col("w2").gt(col("w3")))
                .and(col("w3").gt(col("w4"))),
        )
        .select([col("order_book_id")]);

    // 月线：最近 5 个月收盘严格递增（连续 4 个月上涨）
    let monthly_keys = lf
        .clone()
        .with_columns([col("d").dt().strftime("%Y-%m").alias("ym")]);
    let monthly_hit = monthly_keys
        .sort(
            ["order_book_id", "ym", "d"],
            SortMultipleOptions::default().with_order_descending_multi([false, false, false]),
        )
        .group_by([col("order_book_id"), col("ym")])
        .agg([
            col("close").last().alias("m_close"),
            col("d").last().alias("m_end"),
        ])
        .sort(
            ["order_book_id", "m_end"],
            SortMultipleOptions::default().with_order_descending_multi([false, false]),
        )
        .with_columns([
            col("m_close")
                .shift(lit(1i64))
                .over([col("order_book_id")])
                .alias("m1"),
            col("m_close")
                .shift(lit(2i64))
                .over([col("order_book_id")])
                .alias("m2"),
            col("m_close")
                .shift(lit(3i64))
                .over([col("order_book_id")])
                .alias("m3"),
            col("m_close")
                .shift(lit(4i64))
                .over([col("order_book_id")])
                .alias("m4"),
            col("m_end")
                .max()
                .over([col("order_book_id")])
                .alias("last_m_end"),
        ])
        .filter(col("m_end").eq(col("last_m_end")))
        .filter(
            col("m_close")
                .gt(col("m1"))
                .and(col("m1").gt(col("m2")))
                .and(col("m2").gt(col("m3")))
                .and(col("m3").gt(col("m4"))),
        )
        .select([col("order_book_id")]);

    let out = daily_hit
        .join(
            weekly_hit,
            [col("order_book_id")],
            [col("order_book_id")],
            JoinArgs::new(JoinType::Inner),
        )
        .join(
            monthly_hit,
            [col("order_book_id")],
            [col("order_book_id")],
            JoinArgs::new(JoinType::Inner),
        )
        .sort(
            ["order_book_id"],
            SortMultipleOptions::default().with_order_descending_multi([false]),
        )
        .collect()
        .expect("screen collect");

    println!("匹配股票数: {}", out.height());
    println!("{}", out);
}
