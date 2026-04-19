#[cfg(feature = "openctp")]
use std::env;

#[cfg(feature = "openctp")]
use actix::Actor;

#[cfg(feature = "openctp")]
use qapro_rs::qamarket::live_types::MarketDataSource;
#[cfg(feature = "openctp")]
use qapro_rs::qamarket::qamdgateway::MarketDataDistributor;
#[cfg(feature = "openctp")]
use qapro_rs::qamarket::qareal::{
    start_ctp_md_pump, CTPMdSource, OpenCtpConfig, OpenCtpRuntimeConfig,
};

#[cfg(feature = "openctp")]
fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

#[cfg(feature = "openctp")]
#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let distributor = MarketDataDistributor::new().start();

    let mut source = CTPMdSource::new(OpenCtpConfig {
        md_front: env_or("OPENCTP_MD_FRONT", "tcp://121.37.80.177:20004"),
        td_front: env_or("OPENCTP_TD_FRONT", "tcp://121.37.80.177:20002"),
        broker_id: env_or("OPENCTP_BROKER_ID", "9999"),
        user_id: env_or("OPENCTP_USER_ID", ""),
        password: env_or("OPENCTP_PASSWORD", ""),
        app_id: env::var("OPENCTP_APP_ID").ok(),
        auth_code: env::var("OPENCTP_AUTH_CODE").ok(),
        flow_path: env_or("OPENCTP_FLOW_PATH", "./.cache/openctp"),
    })
    .with_runtime_config(OpenCtpRuntimeConfig {
        md_dynlib_path: env_or("OPENCTP_MD_DYNLIB", ""),
        td_dynlib_path: env_or("OPENCTP_TD_DYNLIB", ""),
        flow_dir: env_or("OPENCTP_FLOW_PATH", "./.cache/openctp"),
        broker_id: env_or("OPENCTP_BROKER_ID", "9999"),
        user_id: env_or("OPENCTP_USER_ID", ""),
        password: env_or("OPENCTP_PASSWORD", ""),
        app_id: env::var("OPENCTP_APP_ID").ok(),
        auth_code: env::var("OPENCTP_AUTH_CODE").ok(),
        use_tts: true,
    });

    let instruments = env::var("OPENCTP_INSTRUMENTS")
        .unwrap_or_else(|_| "ag2604,fu2605".to_string())
        .split(',')
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.trim().to_string())
        .collect::<Vec<_>>();
    source
        .subscribe(&instruments)
        .map_err(std::io::Error::other)?;

    let _pump = start_ctp_md_pump(source, distributor, 20).map_err(std::io::Error::other)?;

    println!("openctp md gateway started, instruments={:?}", instruments);
    loop {
        actix_rt::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

#[cfg(not(feature = "openctp"))]
fn main() {
    eprintln!("please enable `openctp` feature to run this example");
}
