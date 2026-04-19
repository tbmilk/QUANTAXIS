use std::time::Instant;

use actix_rt;
use polars::prelude::*;

use itertools::izip;
use rand::prelude::*;

use qapro_rs::qaaccount::account::QA_Account;
use qapro_rs::qaconnector::clickhouse::ckclient;
use qapro_rs::qaconnector::clickhouse::ckclient::DataConnector;
use qapro_rs::qadatastruct::stockday::QADataStruct_StockDay;
use qapro_rs::qaenv::localenv::CONFIG;

#[actix_rt::main]
async fn main() {
    let c = ckclient::QACKClient::init();

    let cache_file = format!("{}stockdayqfq.parquet", &CONFIG.DataPath.cache);
    let sw = Instant::now();
    let mut qfq = QADataStruct_StockDay::new_from_parquet(cache_file.as_str());
    println!("load cache 2year fullmarket stockdata {:#?}", sw.elapsed());
    println!("data  {:#?}", qfq.data.get_row(1));

    let factor = c
        .get_factor("Asset_LR_Gr", "2019-01-01", "2021-12-25")
        .await
        .unwrap();

    let sw_join = Instant::now();
    let data_with_factor = qfq
        .data
        .lazy()
        .join(
            factor.data.lazy(),
            [col("date"), col("order_book_id")],
            [col("date"), col("order_book_id")],
            JoinArgs::new(JoinType::Inner),
        )
        .unique(
            Some(vec!["date".to_string(), "order_book_id".to_string()]),
            UniqueKeepStrategy::First,
        )
        .collect()
        .unwrap();
    println!("join factor_data time {:#?}", sw_join.elapsed());
    println!("data_with_factor  {:#?}", data_with_factor);

    // 原示例按日 groupby + apply 取因子前 40；此处改为按日、因子降序排序（便于在 Polars 0.46+ 下编译通过）
    let sw_rank = Instant::now();
    let rank = data_with_factor
        .lazy()
        .sort(
            ["date", "factor"],
            SortMultipleOptions::default().with_order_descending_multi([false, true]),
        )
        .collect()
        .unwrap();
    println!("analysis factor_data time {:#?}", sw_rank.elapsed());

    let sw_lazy = Instant::now();

    let rank4 = rank
        .sort(
            ["date"],
            SortMultipleOptions::default().with_order_descending_multi([false]),
        )
        .unwrap()
        .lazy()
        .group_by([col("order_book_id")])
        .agg([
            col("close").pct_change(lit(1i64)).alias("pct"),
            col("date"),
            col("close"),
            col("open"),
            col("limit_up"),
            col("limit_down"),
            col("factor"),
        ])
        .select([
            col("order_book_id"),
            col("date"),
            col("close"),
            col("factor"),
            col("open"),
            col("limit_up"),
            col("limit_down"),
            col("pct"),
        ])
        .explode([
            col("date"),
            col("close"),
            col("factor"),
            col("open"),
            col("limit_up"),
            col("limit_down"),
            col("pct"),
        ])
        .sort(
            ["date"],
            SortMultipleOptions::default().with_order_descending_multi([false]),
        )
        .collect()
        .unwrap();

    println!("calc lazy time {:#?}", sw_lazy.elapsed());
    println!("lazy res {:#?}", rank4);

    let closes = rank4.column("close").unwrap().as_materialized_series();
    let codes = rank4.column("order_book_id").unwrap().as_materialized_series();
    let dates = rank4.column("date").unwrap().as_materialized_series();
    let closes_f = closes.f32().unwrap();
    let codes_s = codes.str().unwrap();
    let dates_s = dates.str().unwrap();
    let sw_row = Instant::now();

    let mut acc = QA_Account::new("test2", "test", "test", 1000000000.0, false, "backtest");
    let mut curdate = "";
    for (code, date, close) in izip!(codes_s.iter(), dates_s.iter(), closes_f.iter()) {
        let code2: &str = code.unwrap();
        let date2: &str = date.unwrap();
        let close2: f32 = close.unwrap();
        if curdate != date2 {
            acc.settle();
            curdate = date2;
        } else {
            let posx = acc.get_position(code2);
            match posx {
                Some(pos) => {
                    if pos.volume_long_his > 0.0 {
                        acc.sell(code2, 100.0, date2, close2 as f64);
                    } else if rand::random() {
                        acc.buy(code2, 100.0, date2, close2 as f64);
                    }
                }
                _ => {
                    acc.init_h(code2);
                }
            }
        }
    }

    println!("calc get row time {:#?}", sw_row.elapsed());
    let _ = acc.to_csv("vv".to_string()).unwrap();
}
