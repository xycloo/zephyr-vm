use std::{env, sync::Arc, time::Duration};

use ingestion_event_catchup::{caching::CacheClient, jobs_manager::JobsManager, ExecutionMode, ExecutionWrapper, FunctionRequest, InvokeZephyrFunction};
use serde_json::json;
use warp::{reject::Rejection, reply::WithStatus, Filter};

fn with_store(
    store: Arc<JobsManager>,
) -> impl Filter<Extract = (Arc<JobsManager>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || store.clone())
}

#[tokio::main]
async fn main() {
    let manager = Arc::new(JobsManager::new());

    let execute = warp::path("execute")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_store(manager.clone()))
        .and_then(
            |body: FunctionRequest, store: Arc<JobsManager>| async move {
                let body_cloned = body.clone();
                let handle = tokio::spawn(async {
                    let execution =
                        ExecutionWrapper::new(body_cloned, env::var("NETWORK").unwrap());
                    let resp = execution.catchup_spawn_jobs().await;

                    resp
                });

                let resp = handle.await.unwrap();
                if resp.is_err() {
                    return Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(
                        "No program avalable".into(),
                        warp::http::StatusCode::BAD_REQUEST,
                    ))
                }
                let resp = resp.unwrap();

                let resp = if body.needs_job() {
                    let job_idx = store.add_job(resp).await;

                    format!("catchup {} in progress", job_idx)
                } else {
                    let response = resp.await.unwrap_or(json!({"error": "code execution trapped."}).to_string());

                    if let ExecutionMode::Function(InvokeZephyrFunction {fname, ..}) = body.mode {
                        if fname == "dashboard" {
                            let cache = CacheClient::new();
                            let _ = cache.insert_or_update(body.binary_id, &response);
                        }
                    }
                    
                    response
                };

                Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(
                    resp,
                    warp::http::StatusCode::OK,
                ))
            },
        );

    let dashboard = warp::path!("dashboard" / u32)
        .and(warp::get())
        .and(with_store(manager.clone()))
        .and_then(|program: u32, _: Arc<JobsManager>| async move {
            /*let handle = tokio::spawn(async move {
                let execution =
                    ExecutionWrapper::new(FunctionRequest::dashboard(program, id), env::var("NETWORK").unwrap());
                let resp = execution.catchup_spawn_jobs().await;

                resp
            });

            let resp = handle.await.unwrap();
                if resp.is_err() {
                    return Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(
                        "No program avalable".into(),
                        warp::http::StatusCode::BAD_REQUEST,
                    ))
                }
            let resp = resp.unwrap();

            let resp = resp.await.unwrap_or("failed".into());*/
                
            let cache = CacheClient::new();
            let result = cache.get_cahched(program);

            match result {
                Ok(response) => Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(
                    response,
                    warp::http::StatusCode::OK,
                )),
                Err(_) => Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(
                    "Error in retrieving the dashboard".into(),
                    warp::http::StatusCode::BAD_REQUEST,
                )) 
            }
        });

    let fetch = warp::path!("catchups" / u32)
        .and(warp::get())
        .and(with_store(manager.clone()))
        .and_then(|id: u32, store: Arc<JobsManager>| async move {
            let status = store.read_job(id).await.unwrap_or("not complete".into());
            Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(
                status,
                warp::http::StatusCode::OK,
            ))
        });

    let routes = warp::post().and(execute).or(fetch).or(dashboard);

    let warp_server =
        tokio::spawn(async move { warp::serve(routes).run(([0, 0, 0, 0], 8085)).await });

    let _ = warp_server.await;
    //jobs_manager.await;
}
