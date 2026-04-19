#![allow(non_camel_case_types, dead_code)]
use polars::prelude::*;
use std::fs::File;
use std::sync::Arc;

use crate::qaenv::localenv::CONFIG;

pub struct QADataStruct_StockDay {
    pub data: DataFrame,
}

fn qa_schema_stock_day() -> Schema {
    use DataType::*;
    vec![
        Field::new("date".into(), String),
        Field::new("code".into(), String),
        Field::new("order_book_id".into(), String),
        Field::new("num_trades".into(), Float32),
        Field::new("limit_up".into(), Float32),
        Field::new("limit_down".into(), Float32),
        Field::new("open".into(), Float32),
        Field::new("high".into(), Float32),
        Field::new("low".into(), Float32),
        Field::new("close".into(), Float32),
        Field::new("volume".into(), Float32),
        Field::new("total_turnover".into(), Float32),
        Field::new("amount".into(), Float32),
    ]
    .into_iter()
    .collect()
}

impl QADataStruct_StockDay {
    pub fn new_from_csv(path: &str) -> Self {
        let schema = qa_schema_stock_day();
        let df = CsvReadOptions::default()
            .with_has_header(true)
            .with_schema(Some(Arc::new(schema)))
            .try_into_reader_with_file_path(Some(path.into()))
            .expect("Cannot open CSV")
            .finish()
            .expect("CSV parse failed");
        Self { data: df }
    }

    fn new_from_path() -> Self {
        let path = format!("{}stockday.parquet", CONFIG.DataPath.cache);
        Self::new_from_parquet(&path)
    }

    pub fn new_from_vec(
        date: Vec<String>,
        code: Vec<String>,
        open: Vec<f32>,
        high: Vec<f32>,
        low: Vec<f32>,
        close: Vec<f32>,
        limit_up: Vec<f32>,
        limit_down: Vec<f32>,
        num_trades: Vec<f32>,
        volume: Vec<f32>,
        total_turnover: Vec<f32>,
    ) -> Self {
        let date_s = Series::new("date".into(), date);
        let order_book_id_s = Series::new("order_book_id".into(), code.clone());
        let code_s = Series::new("code".into(), code);
        let num_trades_s = Series::new("num_trades".into(), num_trades);
        let limit_up_s = Series::new("limit_up".into(), limit_up);
        let limit_down_s = Series::new("limit_down".into(), limit_down);
        let open_s = Series::new("open".into(), open);
        let high_s = Series::new("high".into(), high);
        let low_s = Series::new("low".into(), low);
        let close_s = Series::new("close".into(), close);
        let volume_s = Series::new("volume".into(), volume);
        let total_turnover_s = Series::new("total_turnover".into(), total_turnover.clone());
        let amount_s = Series::new("amount".into(), total_turnover);

        let df = DataFrame::new(vec![
            date_s.into(),
            code_s.into(),
            order_book_id_s.into(),
            num_trades_s.into(),
            limit_up_s.into(),
            limit_down_s.into(),
            open_s.into(),
            high_s.into(),
            low_s.into(),
            close_s.into(),
            volume_s.into(),
            total_turnover_s.into(),
            amount_s.into(),
        ])
        .unwrap();
        let sorted = df
            .sort(
                ["date", "order_book_id"],
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

    pub fn query_code(&mut self, order_book_id: &str) -> DataFrame {
        let s = self
            .data
            .column("order_book_id")
            .expect("column")
            .as_materialized_series();
        let mask = s.equal(order_book_id).expect("compare");
        self.data.filter(&mask).unwrap().clone()
    }

    pub fn query_date(&mut self, date: &str) -> DataFrame {
        let s = self
            .data
            .column("date")
            .expect("column")
            .as_materialized_series();
        let mask = s.equal(date).expect("compare");
        self.data.filter(&mask).unwrap().clone()
    }

    pub fn high(&mut self) -> Series {
        self.data.column("high").unwrap().as_materialized_series().clone()
    }

    pub fn low(&mut self) -> Series {
        self.data.column("low").unwrap().as_materialized_series().clone()
    }

    pub fn close(&mut self) -> Series {
        self.data.column("close").unwrap().as_materialized_series().clone()
    }

    pub fn save_cache(&mut self) {
        let cachepath = format!("{}stockday.parquet", &CONFIG.DataPath.cache);
        let file = File::create(cachepath).expect("could not create file");
        ParquetWriter::new(file)
            .finish(&mut self.data)
            .expect("parquet write");
    }

    pub fn save_selfdefined_cache(&mut self, path: &str) {
        let file = File::create(path).expect("could not create file");
        ParquetWriter::new(file)
            .finish(&mut self.data)
            .expect("parquet write");
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[ignore = "requires testdata.csv"]
    fn test_QADataStruct_StockDay() {
        let mut sd = QADataStruct_StockDay::new_from_csv("testdata.csv");

        println!("Final DataFrame:\n{}", sd.data);
        let high = sd.high();
        let low = sd.low();

        let calc = (&high - &low).unwrap();
        println!("Final Series high - low :\n{}", calc);

        // diff(n) = series - series.shift(n)
        let diff_s = (&high - &high.shift(2)).unwrap();
        println!("High diff:\n{}", diff_s);

        let opts = RollingOptionsFixedWindow {
            window_size: 3,
            min_periods: 1,
            weights: None,
            center: false,
            fn_params: None,
        };
        println!(
            "High rollingstd:\n{}",
            high.rolling_std(opts).unwrap()
        );
    }

    #[test]
    fn test_QADataStruct_StockDay_fromvec() {
        let testds = QADataStruct_StockDay::new_from_vec(
            vec!["2021-01-01".to_string(), "2021-01-02".to_string()],
            vec!["000001.XSHE".to_string(), "000001.XSHE".to_string()],
            vec![20.1, 20.2],
            vec![22.1, 21.1],
            vec![19.2, 19.8],
            vec![21.0, 20.4],
            vec![22.0, 23.0],
            vec![19.0, 19.5],
            vec![99.2, 99.2],
            vec![880.2, 990.2],
            vec![8880.2, 8890.2],
        );
        println!("{:#?}", testds.data);
    }

    #[ignore = "requires CONFIG/infrastructure"]
    #[test]
    fn test_QADataStruct_StockDay_save() {
        let mut testds = QADataStruct_StockDay::new_from_vec(
            vec!["2021-01-01".to_string(), "2021-01-02".to_string()],
            vec!["000001.XSHE".to_string(), "000001.XSHE".to_string()],
            vec![20.1, 20.2],
            vec![22.1, 21.1],
            vec![19.2, 19.8],
            vec![21.0, 20.4],
            vec![22.0, 23.0],
            vec![19.0, 19.5],
            vec![99.2, 99.2],
            vec![880.2, 990.2],
            vec![8880.2, 8890.2],
        );
        println!("{:#?}", testds.data);

        testds.save_cache();
    }
}
