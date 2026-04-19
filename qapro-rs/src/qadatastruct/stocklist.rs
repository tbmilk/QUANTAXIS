#![allow(non_camel_case_types, dead_code)]
use polars::prelude::*;

pub struct QADataStruct_StockList {
    pub data: DataFrame,
    name: String,
}

impl QADataStruct_StockList {
    pub fn new_from_vec(
        order_book_id: Vec<String>,
        listed_date: Vec<String>,
        delist_date: Vec<String>,
        symbol: Vec<String>,
    ) -> Self {
        let order_book_id_s = Series::new("order_book_id".into(), order_book_id);
        let listed_date_s = Series::new("listed_date".into(), listed_date);
        let delist_date_s = Series::new("delist_date".into(), delist_date);
        let symbol_s = Series::new("symbol".into(), symbol);
        let df = DataFrame::new(vec![
            order_book_id_s.into(),
            listed_date_s.into(),
            delist_date_s.into(),
            symbol_s.into(),
        ])
        .unwrap();

        QADataStruct_StockList {
            data: df,
            name: "stocklist".to_string(),
        }
    }
}
