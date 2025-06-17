use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use opentelemetry::KeyValue;
use opentelemetry_sdk::metrics::PeriodicReader;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::runtime::Tokio;

// 2022-2024 (c) Copyright Contributors to the GOSH DAO. All rights reserved.
//
pub mod mpsc;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

pub fn millis_from_now(start_ms: u64) -> Result<u64, String> {
    let now = now_ms();
    if now >= start_ms {
        Ok(now - start_ms)
    } else if start_ms - now < 5 {
        // we think that a 5ms difference in clock synchronization is acceptable
        Ok(0)
    } else {
        Err("System clock out of sync. Please check NTP or system time settings.".to_string())
    }
}

pub fn init_meter_provider() -> SdkMeterProvider {
    let default_service_name = KeyValue::new("service.name", "acki-nacki-node");

    let resource = opentelemetry_sdk::Resource::new(vec![default_service_name.clone()])
        .merge(&opentelemetry_sdk::Resource::default());

    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .build()
        .expect("Failed to build OTLP metrics exporter");

    SdkMeterProvider::builder()
        .with_reader(
            PeriodicReader::builder(metric_exporter, Tokio)
                .with_interval(Duration::from_secs(30))
                .with_timeout(Duration::from_secs(5))
                .build(),
        )
        .with_resource(resource)
        .build()
}
