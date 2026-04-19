#![allow(non_camel_case_types, dead_code)]
use polars::prelude::*;
use std::fs::File;
use std::sync::Arc;

use crate::qaenv::localenv::CONFIG;

/// 期货日线数据结构
pub struct QADataStruct_FutureDay {
    pub data: DataFrame,
}

fn qa_schema_future_day() -> Schema {
    use DataType::*;
    vec![
        Field::new("date".into(), String),
        Field::new("code".into(), String),
        Field::new("open".into(), Float32),
        Field::new("high".into(), Float32),
        Field::new("low".into(), Float32),
        Field::new("close".into(), Float32),
        Field::new("volume".into(), Float32),
        Field::new("amount".into(), Float32),
        Field::new("open_interest".into(), Float32),
        Field::new("settlement".into(), Float32),
        Field::new("pre_settlement".into(), Float32),
        Field::new("upper_limit".into(), Float32),
        Field::new("lower_limit".into(), Float32),
    ]
    .into_iter()
    .collect()
}

impl QADataStruct_FutureDay {
    pub fn new_from_csv(path: &str) -> Self {
        let schema = qa_schema_future_day();
        let df = CsvReadOptions::default()
            .with_has_header(true)
            .with_schema(Some(Arc::new(schema)))
            .try_into_reader_with_file_path(Some(path.into()))
            .expect("Cannot open CSV")
            .finish()
            .expect("CSV parse failed");
        Self { data: df }
    }

    pub fn new_from_vec(
        date: Vec<String>,
        code: Vec<String>,
        open: Vec<f32>,
        high: Vec<f32>,
        low: Vec<f32>,
        close: Vec<f32>,
        volume: Vec<f32>,
        amount: Vec<f32>,
        open_interest: Vec<f32>,
        settlement: Vec<f32>,
    ) -> Self {
        let df = DataFrame::new(vec![
            Series::new("date".into(), date).into(),
            Series::new("code".into(), code).into(),
            Series::new("open".into(), open).into(),
            Series::new("high".into(), high).into(),
            Series::new("low".into(), low).into(),
            Series::new("close".into(), close).into(),
            Series::new("volume".into(), volume).into(),
            Series::new("amount".into(), amount).into(),
            Series::new("open_interest".into(), open_interest).into(),
            Series::new("settlement".into(), settlement).into(),
        ])
        .unwrap();
        let sorted = df
            .sort(
                ["date", "code"],
                SortMultipleOptions::default().with_order_descending_multi([false, false]),
            )
            .unwrap();
        Self { data: sorted }
    }

    pub fn new_from_parquet(path: &str) -> Self {
        let file = File::open(path).expect("Cannot open file.");
        let df = ParquetReader::new(file).finish().unwrap();
        Self { data: df }
    }

    pub fn close(&self) -> Series {
        self.data.column("close").unwrap().as_materialized_series().clone()
    }

    pub fn open_interest(&self) -> Series {
        self.data.column("open_interest").unwrap().as_materialized_series().clone()
    }

    pub fn query_code(&mut self, code: &str) -> DataFrame {
        let mask = self
            .data
            .column("code")
            .unwrap()
            .as_materialized_series()
            .equal(code)
            .unwrap();
        self.data.filter(&mask).unwrap()
    }

    pub fn save_cache(&mut self) {
        let cachepath = format!("{}futureday.parquet", &CONFIG.DataPath.cache);
        let file = File::create(cachepath).expect("could not create file");
        ParquetWriter::new(file).finish(&mut self.data).expect("parquet write");
    }
}
