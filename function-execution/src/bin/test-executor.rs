use std::fs::read;

use function_execution::{ExecutionWrapper, FunctionRequest};
use stellar_xdr::next::{Hash, Limits, ReadXdr};
use warp::{reject::Rejection, reply::WithStatus, Filter};

#[tokio::main]
async fn main() {
    let execute = warp::path("run").and(warp::post()).and(warp::body::json()).and_then(|body: FunctionRequest| async move {
        let code = { read("/mnt/storagehdd/projects/master/zephyr/target/wasm32-unknown-unknown/release/entries_filter.wasm").unwrap() };
        let execution = ExecutionWrapper::new(&code);

        let res = execution.reproduce_async_runtime(&body.fname).await;

        Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(res, warp::http::StatusCode::OK))
    });

    let routes = warp::post().and(execute);

    warp::serve(routes).run(([0, 0, 0, 0], 8443)).await
}

#[test]
fn test() {
    println!(
        "{:?}",
        Hash::from_xdr_base64(
            "/5cuUQJvhjybwqW9LLMyNCgQCZXhpfFTptnbbkIbn+8=",
            Limits::none()
        )
    );
}
