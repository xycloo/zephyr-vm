use std::fs::read;
use ingestion_event_catchup::ExecutionWrapper;

#[tokio::main]
async fn main() {
    let code = { read(std::env::var("TARGET").unwrap()).unwrap() };
    let execution = ExecutionWrapper::new(&code);
    let _ = execution.spawn_jobs("on_close").await;
}