use actix_rt;

use qapro_rs::qaconnector::clickhouse::ckclient;
use qapro_rs::qaconnector::clickhouse::ckclient::DataConnector;
#[actix_rt::main]
async fn main() {
    let c = ckclient::QACKClient::init();
    let factor = c
        .get_factor("Asset_LR_Gr", "2021-01-01", "2021-10-01")
        .await
        .unwrap();
    println!("{:#?}", factor.data);
}
