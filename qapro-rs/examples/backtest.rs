use std::time::Instant;

use actix_rt;
use polars::prelude::*;

use itertools::izip;
use rand::prelude::*;

use qapro_rs::qaaccount::account::QA_Account;
use qapro_rs::qadatastruct::stockday::QADataStruct_StockDay;
use qapro_rs::qaenv::localenv::CONFIG;

#[actix_rt::main]
async fn main() {
    let cache_file = format!("{}stockdayqfq.parquet", &CONFIG.DataPath.cache);
    let sw = Instant::now();
    let mut qfq = QADataStruct_StockDay::new_from_parquet(cache_file.as_str());
    println!("load cache 2year fullmarket stockdata {:#?}", sw.elapsed());
    println!("data  {:#?}", qfq.data.get_row(1));

    let sw2 = Instant::now();
    let rank4 = qfq
        .data
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
        ])
        .select([
            col("order_book_id"),
            col("date"),
            col("close"),
            col("open"),
            col("limit_up"),
            col("limit_down"),
            col("pct"),
        ])
        .explode([
            col("date"),
            col("close"),
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

    println!("calc lazy time {:#?}", sw2.elapsed());
    println!("lazy res {:#?}", rank4);

    let closes = rank4.column("close").unwrap().as_materialized_series();
    let codes = rank4.column("order_book_id").unwrap().as_materialized_series();
    let dates = rank4.column("date").unwrap().as_materialized_series();
    let closes_f = closes.f32().unwrap();
    let codes_s = codes.str().unwrap();
    let dates_s = dates.str().unwrap();

    let mut acc = QA_Account::new("test", "test", "test", 1000000000.0, false, "backtest");
    let mut curdate = "";
    let sw3 = Instant::now();

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
    let _ = acc.to_csv("".to_string());

    println!("calc get row time {:#?}", sw3.elapsed());
}
