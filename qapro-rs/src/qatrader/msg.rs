use serde::{Deserialize, Serialize};
use serde_json::Value;
use log::{info, error};

#[derive(Serialize, Deserialize, Debug)]
pub struct Peek {
    pub aid: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Broker {
    pub aid: String,
    pub brokers: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReqLogin {
    pub aid: String,
    pub bid: String,
    pub user_name: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReqOrder {
    pub aid: String,
    pub user_id: String,
    pub order_id: String,
    pub exchange_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: i64,
    pub price_type: String,
    pub limit_price: f64,
    pub volume_condition: String,
    pub time_condition: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReqCancel {
    pub aid: String,
    pub user_id: String,
    pub order_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReqQueryBank {
    pub aid: String,
    pub bank_id: String,
    pub future_account: String,
    pub future_password: String,
    pub bank_password: String,
    pub currency: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReqQuerySettlement {
    pub aid: String,
    pub trading_day: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReqChangePassword {
    pub aid: String,
    pub old_password: String,
    pub new_password: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReqTransfer {
    pub aid: String,
    pub bank_id: String,
    pub future_account: String,
    pub future_password: String,
    pub bank_password: String,
    pub currency: String,
    pub amount: f64,
}

fn get_string_field(data: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| data.get(*key).and_then(|value| value.as_str()))
        .map(|value| value.to_string())
}

pub fn parse_message(msg: String) -> Option<String> {
    let resx: Value = match serde_json::from_str(&msg) {
        Ok(data) => data,
        Err(e) => {
            error!("{:?}", e);
            return None;
        }
    };
    let topic = resx["topic"].as_str()?;
    let data = match topic {
        "sendorder" => {
            let order_id = resx["order_id"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let order = ReqOrder {
                aid: "insert_order".to_string(),
                user_id: resx["account_cookie"].as_str()?.to_string(),
                order_id,
                exchange_id: get_string_field(&resx, &["exchange_id"])?,
                instrument_id: get_string_field(&resx, &["instrument_id", "code"])?,
                direction: get_string_field(&resx, &["direction", "order_direction"])?,
                offset: get_string_field(&resx, &["offset", "order_offset"])?,
                volume: resx["volume"].as_f64()? as i64,
                price_type: "LIMIT".to_string(),
                limit_price: resx["price"].as_f64()?,
                volume_condition: "ANY".to_string(),
                time_condition: "GFD".to_string(),
            };
            let b = serde_json::to_string(&order).ok()?;
            info!("[send order] {:?}", b);
            b
        }
        "cancel_order" => {
            let cancel = ReqCancel {
                aid: "cancel_order".to_string(),
                user_id: resx["account_cookie"].as_str()?.to_string(),
                order_id: resx["order_id"].as_str()?.to_string(),
            };
            serde_json::to_string(&cancel).ok()?
        }
        "transfer" => {
            let t = ReqTransfer {
                aid: "req_transfer".to_string(),
                bank_id: resx["bank_id"].as_str()?.to_string(),
                future_account: resx["account_cookie"].as_str()?.to_string(),
                future_password: resx["future_password"].as_str()?.to_string(),
                bank_password: resx["bank_password"].as_str()?.to_string(),
                currency: "CNY".to_string(),
                amount: resx["amount"].as_f64()?,
            };
            serde_json::to_string(&t).ok()?
        }
        "query_settlement" => {
            let q = ReqQuerySettlement {
                aid: "qry_settlement_info".to_string(),
                trading_day: resx["trading_day"].as_i64()?,
            };
            serde_json::to_string(&q).ok()?
        }
        "query_bank" => {
            let q = ReqQueryBank {
                aid: "qry_bankcapital".to_string(),
                bank_id: resx["bank_id"].as_str()?.to_string(),
                future_account: resx["account_cookie"].as_str()?.to_string(),
                future_password: resx["future_password"].as_str()?.to_string(),
                bank_password: resx["bank_password"].as_str()?.to_string(),
                currency: "CNY".to_string(),
            };
            serde_json::to_string(&q).ok()?
        }
        "change_password" => {
            let c = ReqChangePassword {
                aid: "change_password".to_string(),
                old_password: resx["old_password"].as_str()?.to_string(),
                new_password: resx["new_password"].as_str()?.to_string(),
            };
            serde_json::to_string(&c).ok()?
        }
        "peek" => {
            let p = Peek { aid: "peek_message".to_string() };
            serde_json::to_string(&p).ok()?
        }
        "login" => {
            let l = ReqLogin {
                aid: "req_login".to_string(),
                bid: resx["bid"].as_str()?.to_string(),
                user_name: resx["user_name"].as_str()?.to_string(),
                password: resx["password"].as_str()?.to_string(),
            };
            serde_json::to_string(&l).ok()?
        }
        _ => {
            error!("[Unknown Topic!] {:?}", resx);
            return None;
        }
    };
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_message_prefers_instrument_id() {
        let raw = r#"{
            "topic":"sendorder",
            "account_cookie":"acc-1",
            "exchange_id":"SHFE",
            "instrument_id":"ag2604",
            "code":"legacy-code",
            "direction":"BUY",
            "offset":"OPEN",
            "volume":2,
            "price":5100.5
        }"#;

        let parsed = parse_message(raw.to_string()).unwrap();
        let order: ReqOrder = serde_json::from_str(&parsed).unwrap();
        assert_eq!(order.instrument_id, "ag2604");
        assert_eq!(order.direction, "BUY");
        assert_eq!(order.offset, "OPEN");
    }

    #[test]
    fn test_parse_message_accepts_legacy_code_fields() {
        let raw = r#"{
            "topic":"sendorder",
            "account_cookie":"acc-1",
            "exchange_id":"SHFE",
            "code":"ag2604",
            "order_direction":"BUY",
            "order_offset":"OPEN",
            "volume":2,
            "price":5100.5
        }"#;

        let parsed = parse_message(raw.to_string()).unwrap();
        let order: ReqOrder = serde_json::from_str(&parsed).unwrap();
        assert_eq!(order.instrument_id, "ag2604");
        assert_eq!(order.direction, "BUY");
        assert_eq!(order.offset, "OPEN");
    }
}
