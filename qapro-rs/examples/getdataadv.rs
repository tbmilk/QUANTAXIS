use actix_rt;

use polars::prelude::*;
use polars::series::ops::NullBehavior;

use qapro_rs::qaconnector::clickhouse::ckclient;
use qapro_rs::qaconnector::clickhouse::ckclient::DataConnector;
use qapro_rs::qadatastruct::stockday::QADataStruct_StockDay;
use qapro_rs::qaenv::localenv::CONFIG;

#[actix_rt::main]
async fn main() {
    let c = ckclient::QACKClient::init();

    let start = CONFIG.DataPath.cachestart.as_str();
    let end = CONFIG.DataPath.cacheend.as_str();
    let stocklist = c.get_stocklist().await.unwrap();

    let stocklistvec: Vec<&str> = stocklist.iter().map(|x| x.as_str()).collect();

    let mut hisdata = c
        .get_stock_adv(stocklistvec.clone(), start, end, "day")
        .await
        .unwrap();

    println!("qadatastruct {}", hisdata.data);
    hisdata.save_cache();

    let mut adj = c
        .get_stock_adj(stocklistvec.clone(), "2019-01-01", "2021-12-22")
        .await
        .unwrap();
    println!("adj  {:#?}", adj.data);
    adj.save_cache();

    let cache_file = format!("{}stockday.parquet", &CONFIG.DataPath.cache);

    let mut data = QADataStruct_StockDay::new_from_parquet(cache_file.as_str());

    println!("load cache file {:#?}", data.data);

    println!(
        "groupby test {:#?}",
        data.data
            .clone()
            .lazy()
            .group_by([col("date")])
            .agg([col("close").mean()])
            .collect()
            .unwrap()
    );
    println!(
        "groupby test {:#?}",
        data.data
            .clone()
            .lazy()
            .group_by([col("order_book_id")])
            .agg([col("close").mean()])
            .collect()
            .unwrap()
    );

    let high_s = data
        .data
        .column("high")
        .unwrap()
        .as_materialized_series();
    // Polars 0.46+: `diff` 为 polars_ops 中的函数，不再在 Series 上作为方法
    println!(
        "diff test {:#?}",
        diff(high_s, 1, NullBehavior::Drop).unwrap()
    );

    let selectdf = data.query_code("300002.XSHE");
    println!("select df {:#?}", selectdf);
    let close = selectdf.column("close").unwrap().as_materialized_series();
    let lastclose = close.shift(1);
    // 避免 `&close` 形成 `&&Series`，与 Polars 的 `Div for &Series` 不匹配
    println!("pct test {:#?}", close / &lastclose);

    let opts = RollingOptionsFixedWindow {
        window_size: 5,
        min_periods: 1,
        weights: None,
        center: false,
        fn_params: None,
    };
    let ma20 = close.rolling_mean(opts).unwrap();
    println!("rolling mean test {:#?}", ma20);
}
