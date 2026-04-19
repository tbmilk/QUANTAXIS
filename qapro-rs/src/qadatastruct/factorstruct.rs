#![allow(non_camel_case_types, dead_code)]
use polars::prelude::*;

pub struct QADataStruct_Factor {
    pub data: DataFrame,
    name: String,
}

impl QADataStruct_Factor {
    pub fn new_from_vec(
        date: Vec<String>,
        order_book_id: Vec<String>,
        factor: Vec<f32>,
        factorname: String,
    ) -> Self {
        let date_s = Series::new("date".into(), date);
        let order_book_id_s = Series::new("order_book_id".into(), order_book_id);
        let factor_s = Series::new("factor".into(), factor);
        let df = DataFrame::new(vec![date_s.into(), order_book_id_s.into(), factor_s.into()]).unwrap();
        let sorted = df
            .sort(
                ["date", "order_book_id"],
                SortMultipleOptions::default().with_order_descending_multi([false, false]),
            )
            .unwrap();
        Self {
            data: sorted,
            name: factorname,
        }
    }
}
