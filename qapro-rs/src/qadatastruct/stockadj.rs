#![allow(non_camel_case_types, dead_code)]
use crate::qaenv::localenv::CONFIG;

use polars::prelude::*;
use std::fs::File;

pub struct QADataStruct_StockAdj {
    pub data: DataFrame,
    name: String,
}

impl QADataStruct_StockAdj {
    pub fn new_from_vec(date: Vec<String>, order_book_id: Vec<String>, adj: Vec<f32>) -> Self {
        let date_s = Series::new("date".into(), date);
        let order_book_id_s = Series::new("order_book_id".into(), order_book_id);
        let adj_s = Series::new("adj".into(), adj);
        let df = DataFrame::new(vec![date_s.into(), order_book_id_s.into(), adj_s.into()]).unwrap();
        let sorted = df
            .sort(
                ["date", "order_book_id"],
                SortMultipleOptions::default().with_order_descending_multi([false, false]),
            )
            .unwrap();
        Self {
            data: sorted,
            name: "stockadj".to_string(),
        }
    }

    pub fn new_from_parquet(path: &str) -> Self {
        let file = File::open(path).expect("Cannot open file.");
        let df = ParquetReader::new(file).finish().unwrap();
        Self {
            data: df,
            name: "stockadj".to_string(),
        }
    }

    pub fn save_cache(&mut self) {
        let cachepath = format!("{}stockadj.parquet", &CONFIG.DataPath.cache);
        let file = File::create(cachepath).expect("could not create file");
        ParquetWriter::new(file)
            .finish(&mut self.data)
            .expect("parquet write");
    }
}
