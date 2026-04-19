//! 从 MongoDB `stock_day` 拉取 QUANTAXIS 已保存的日线，构建 `QADataStruct_StockDay`，便于接 Polars / 回测。
//!
//! 运行（需 Mongo 中有数据，且 `example.toml` 中 `[hisdata]` 指向同一库）：
//! ```text
//! cargo run --example backtest_mongo --release -- /path/to/example.toml
//! ```

use qapro_rs::qaconnector::mongo::stock_day::{
    load_stock_day_from_app_config, order_book_id_from_mongo_code,
};
use qapro_rs::qaenv::localenv::CONFIG;

fn main() {
    // CONFIG 由 `qaenv::localenv` 在首次访问时从 argv 解析 toml
    let _ = &*CONFIG;

    let codes: Vec<String> = vec![
        "000001".to_string(),
        "600000".to_string(),
    ];
    let start = "2024-01-01";
    let end = "2024-06-30";

    match load_stock_day_from_app_config(&codes, start, end) {
        Ok(mut ds) => {
            println!("rows: {}", ds.data.height());
            let id = order_book_id_from_mongo_code("000001");
            println!("sample slice for {}:\n{}", id, ds.query_code(&id));
        }
        Err(e) => eprintln!("load_stock_day_from_app_config error: {}", e),
    }
}
