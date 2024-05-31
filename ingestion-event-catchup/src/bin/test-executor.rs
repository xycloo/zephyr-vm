use std::{env, sync::Arc, time::Duration};

use ingestion_event_catchup::{jobs_manager::JobsManager, ExecutionWrapper, FunctionRequest};
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
                let resp = if body.needs_job() {
                    let job_idx = store.add_job(resp).await;

                    format!("catchup {} in progress", job_idx)
                } else {
                    resp.await.unwrap_or("failed".into())
                };

                Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(
                    resp,
                    warp::http::StatusCode::OK,
                ))
            },
        );

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

    let routes = warp::post().and(execute).or(fetch);

    let warp_server =
        tokio::spawn(async move { warp::serve(routes).run(([0, 0, 0, 0], 8085)).await });

    let _ = warp_server.await;
    //jobs_manager.await;
}
