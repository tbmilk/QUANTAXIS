#![allow(non_camel_case_types, dead_code)]
use polars::prelude::*;
use std::fs::File;

/// 股票板块/行业数据（代码与所属板块的映射）
pub struct QADataStruct_StockBlock {
    pub data: DataFrame,
}

impl QADataStruct_StockBlock {
    /// 从向量构建：code, blockname, source（数据来源，如 "sw"/"zjhhy"/"tdxhy"）
    pub fn new_from_vec(
        code: Vec<String>,
        blockname: Vec<String>,
        source: Vec<String>,
    ) -> Self {
        let df = DataFrame::new(vec![
            Series::new("code".into(), code).into(),
            Series::new("blockname".into(), blockname).into(),
            Series::new("source".into(), source).into(),
        ])
        .unwrap();
        Self { data: df }
    }

    pub fn new_from_parquet(path: &str) -> Self {
        let file = File::open(path).expect("Cannot open file.");
        let df = ParquetReader::new(file).finish().unwrap();
        Self { data: df }
    }

    /// 查询某代码所属的全部板块
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

    /// 查询某板块下的全部代码
    pub fn query_block(&mut self, blockname: &str) -> DataFrame {
        let mask = self
            .data
            .column("blockname")
            .unwrap()
            .as_materialized_series()
            .equal(blockname)
            .unwrap();
        self.data.filter(&mask).unwrap()
    }

    /// 按来源过滤
    pub fn query_source(&mut self, source: &str) -> DataFrame {
        let mask = self
            .data
            .column("source")
            .unwrap()
            .as_materialized_series()
            .equal(source)
            .unwrap();
        self.data.filter(&mask).unwrap()
    }
}
