use ingestion_event_catchup::{ExecutionWrapper, FunctionRequest};
use warp::{reject::Rejection, reply::WithStatus, Filter};

#[tokio::main]
async fn main() {
    let execute = warp::path("execute").and(warp::post()).and(warp::body::json()).and_then(|body: FunctionRequest| async move {
        let handle = tokio::spawn(async {
            let execution = ExecutionWrapper::new(body);
            let resp = execution.catchup_spawn_jobs().await;

            resp
        });

        let resp = handle.await;

        Ok::<WithStatus<String>, Rejection>(warp::reply::with_status(resp.unwrap_or("failed".into()), warp::http::StatusCode::OK))
    });

    let routes = warp::post().and(execute);
    
    warp::serve(routes).run(([0, 0, 0, 0], 8085)).await
}
