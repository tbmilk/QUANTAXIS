extern crate mongodb;

use crate::qaaccount::account::QA_Account;
use crate::qaenv::localenv::CONFIG;
use crate::qaprotocol::qifi::account::QIFI;
use crate::qaprotocol::qifi::func::{from_serde_value, from_string};
use chrono::Local;
use log::{error, warn};

pub use mongodb::{
    bson::{doc, Bson, Document},
    error::Result,
    options::{
        DeleteOptions, FindOneOptions, FindOptions, InsertOneOptions, UpdateModifications,
        UpdateOptions,
    },
    sync::{Client, Collection},
};

use serde::Serialize;

pub fn struct_to_doc<T>(value: T) -> Document
where
    T: Serialize + std::fmt::Debug,
{
    match mongodb::bson::to_bson(&value)
        .ok()
        .and_then(|value| value.as_document().cloned())
    {
        Some(doc) => doc,
        None => {
            error!("[MongoClient] failed to serialize value into BSON document");
            Document::new()
        }
    }
}

#[derive(Debug, Clone)]
pub struct QAMongoClient {
    pub client: Client,
}

impl QAMongoClient {
    pub async fn new(uri: &str) -> Self {
        let client = Client::with_uri_str(uri).unwrap();

        Self { client }
    }
    pub async fn get_qifi(&self, account_cookie: String) -> QIFI {
        let coll = self
            .client
            .database(CONFIG.account.db.as_str())
            .collection("account");
        let cursor = match coll.find_one(doc! {"account_cookie": &account_cookie}, None) {
            Ok(cursor) => cursor,
            Err(err) => {
                error!(
                    "[MongoClient] find_one account_cookie={} failed: {}",
                    account_cookie, err
                );
                return QIFI::default();
            }
        };
        let Some(document) = cursor else {
            warn!(
                "[MongoClient] account_cookie={} not found, returning default QIFI",
                account_cookie
            );
            return QIFI::default();
        };
        let serialized = match serde_json::to_string(&document) {
            Ok(serialized) => serialized,
            Err(err) => {
                error!(
                    "[MongoClient] serialize account_cookie={} failed: {}",
                    account_cookie, err
                );
                return QIFI::default();
            }
        };
        let Some(value) = from_string(serialized) else {
            error!(
                "[MongoClient] parse account_cookie={} JSON payload failed",
                account_cookie
            );
            return QIFI::default();
        };
        match from_serde_value(value) {
            Ok(qifi) => qifi,
            Err(err) => {
                error!(
                    "[MongoClient] deserialize account_cookie={} into QIFI failed: {}",
                    account_cookie, err
                );
                QIFI::default()
            }
        }
    }
    pub async fn get_account(&self, account_cookie: String) -> QA_Account {
        // 转换为结构体
        let c: QIFI = self.get_qifi(account_cookie).await;
        // println!("{:#?}", c);
        QA_Account::new_from_qifi(c)
    }

    pub async fn get_accountlist(&self) -> Vec<String> {
        let coll = self
            .client
            .database(CONFIG.account.db.as_str())
            .collection("account");
        let mut cursor = coll
            .find(
                None,
                FindOptions::builder()
                    .batch_size(Option::from(10000000))
                    .projection(Option::from(doc! {"account_cookie": 1}))
                    .build(),
            )
            .expect("Failed to execute find.");

        let mut u: Vec<String> = Vec::new();
        while let Some(result) = cursor.next() {
            match result {
                Ok(document) => {
                    if let Some(title) = document.get("account_cookie").and_then(Bson::as_str) {
                        u.push(title.to_string());
                    }
                }
                Err(_e) => {}
            }
        }
        u
    }

    pub async fn save_account(&self, mut account: QA_Account) {
        // 实时切片
        let slice: QIFI = account.get_qifi_slice();
        self.save_qifi_slice(slice).await;
    }

    pub async fn save_qifi_slice(&self, mut slice: QIFI) {
        slice.updatetime = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let coll = self
            .client
            .database(CONFIG.account.db.as_str())
            .collection("account");

        let v = struct_to_doc(slice.clone());
        if let Err(err) = coll.update_one(
            doc! {"account_cookie": slice.account_cookie},
            v,
            UpdateOptions::builder().upsert(Option::from(true)).build(),
        ) {
            error!("[MongoClient] save_qifi_slice failed: {}", err);
        }
    }

    pub async fn save_his_qifi_slice(&self, slice: QIFI) {
        let coll = self
            .client
            .database(CONFIG.account.db.as_str())
            .collection("history");
        let trading_day = slice.trading_day.clone();
        let v = struct_to_doc(slice.clone());
        if let Err(err) = coll.update_one(
            doc! {"account_cookie": slice.account_cookie,"trading_day":trading_day},
            v,
            UpdateOptions::builder().upsert(Option::from(true)).build(),
        ) {
            error!("[MongoClient] save_his_qifi_slice failed: {}", err);
        }
    }

    pub async fn save_accounthis(&self, mut account: QA_Account) {
        let slice: QIFI = account.get_qifi_slice();

        let coll = self
            .client
            .database(CONFIG.account.db.as_str())
            .collection("history");
        let trading_day = slice.trading_day.clone();
        let v = struct_to_doc(slice.clone());
        if let Err(err) = coll.update_one(
            doc! {"account_cookie": slice.account_cookie,"trading_day":trading_day},
            v,
            UpdateOptions::builder().upsert(Option::from(true)).build(),
        ) {
            error!("[MongoClient] save_accounthis failed: {}", err);
        }
    }
}
