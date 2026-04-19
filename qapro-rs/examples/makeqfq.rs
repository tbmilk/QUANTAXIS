use actix_rt;
use std::time::Instant;

use polars::prelude::*;

use qapro_rs::qaconnector::clickhouse::ckclient;
use qapro_rs::qaconnector::clickhouse::ckclient::DataConnector;
use qapro_rs::qadatastruct::stockadj::QADataStruct_StockAdj;
use qapro_rs::qadatastruct::stockday::QADataStruct_StockDay;
use qapro_rs::qaenv::localenv::CONFIG;

#[actix_rt::main]
async fn main() {
    let c = ckclient::QACKClient::init();

    let _stocklist = c.get_stocklist().await.unwrap();

    let cache_file = format!("{}stockday.parquet", &CONFIG.DataPath.cache);
    let sw = Instant::now();
    let mut data = QADataStruct_StockDay::new_from_parquet(cache_file.as_str());
    println!("load cache 2year fullmarket stockdata {:#?}", sw.elapsed());

    let cache_file = format!("{}stockadj.parquet", &CONFIG.DataPath.cache);
    let adj = QADataStruct_StockAdj::new_from_parquet(cache_file.as_str());

    let sw2 = Instant::now();
    let qfq = data
        .data
        .lazy()
        .join(
            adj.data.lazy(),
            [col("date"), col("order_book_id")],
            [col("date"), col("order_book_id")],
            JoinArgs::new(JoinType::Inner),
        )
        .with_columns([
            (col("open") * col("adj")).alias("open"),
            (col("high") * col("adj")).alias("high"),
            (col("low") * col("adj")).alias("low"),
            (col("close") * col("adj")).alias("close"),
            (col("limit_up") * col("adj")).alias("limit_up"),
            (col("limit_down") * col("adj")).alias("limit_down"),
        ])
        .drop(["adj"])
        .unique(
            Some(vec!["date".to_string(), "order_book_id".to_string()]),
            UniqueKeepStrategy::First,
        )
        .collect()
        .unwrap();

    println!("run qfq calc {:#?}", sw2.elapsed());
    println!("qfq data {:#?}", qfq);

    let mut qfqstruct = QADataStruct_StockDay { data: qfq };
    qfqstruct
        .save_selfdefined_cache(format!("{}stockdayqfq.parquet", CONFIG.DataPath.cache).as_str());
}
