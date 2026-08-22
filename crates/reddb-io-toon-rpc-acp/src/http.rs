//! REST transport for ACP — agents and runs over HTTP.
//!
//! Endpoints (matching the ACP REST spec):
//!
//! - `GET    /agents`              — list all agents
//! - `POST   /agents/{name}/runs`  — start a new run
//! - `GET    /runs/{id}`          — fetch a run by id
//! - `DELETE /runs/{id}`          — cancel a run
//!
//! Responses are JSON by default. Clients that send `Accept: application/toon`
//! get TOON-encoded responses (the wire format that powers toon-rpc).

use crate::types::{AgentRun, AgentRunInput, AgentSummary, ACP_API_VERSION};
use crate::AcpService;
use http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use parking_lot::Mutex;
use reddb_io_toon::Value as ToonValue;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

type RunsMap = Arc<Mutex<HashMap<String, AgentRun>>>;

/// Run the ACP server over HTTP until the process is killed.
pub async fn serve_http<S: AcpService>(
    service: S,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = Arc::new(service);
    let runs: RunsMap = Arc::new(Mutex::new(HashMap::new()));

    let listener = TcpListener::bind(addr).await?;
    println!("[toon-rpc-acp] HTTP server listening on http://{}", addr);
    println!(
        "[toon-rpc-acp] try: curl -H 'Accept: application/toon' http://{}/agents",
        addr
    );

    loop {
        let (stream, _) = listener.accept().await?;
        let service = service.clone();
        let runs = runs.clone();

        tokio::spawn(async move {
            let connection = hyper_util::rt::TokioIo::new(stream);
            let svc = hyper::service::service_fn(move |req| {
                let svc = service.clone();
                let runs = runs.clone();
                async move { handle_request(req, svc, runs).await }
            });

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(connection, svc)
                .await
            {
                eprintln!("[toon-rpc-acp] connection error: {}", e);
            }
        });
    }
}

async fn handle_request<S: AcpService>(
    req: Request<Incoming>,
    service: Arc<S>,
    runs: RunsMap,
) -> Result<Response<String>, Infallible> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let accept_toon = wants_toon(&req);

    if method == http::Method::GET && path == "/agents" {
        return Ok(handle_list_agents(&*service, accept_toon));
    }

    if method == http::Method::POST && path.starts_with("/agents/") && path.ends_with("/runs") {
        let agent_name = path
            .trim_start_matches("/agents/")
            .trim_end_matches("/runs")
            .to_string();
        return Ok(handle_create_run(req, &*service, runs, &agent_name, accept_toon).await);
    }

    if method == http::Method::GET && path.starts_with("/runs/") {
        let run_id = path.trim_start_matches("/runs/").to_string();
        return Ok(handle_get_run(&runs, &run_id, accept_toon));
    }

    if method == http::Method::DELETE && path.starts_with("/runs/") {
        let run_id = path.trim_start_matches("/runs/").to_string();
        return Ok(handle_cancel_run(&*service, &runs, &run_id, accept_toon));
    }

    if method == http::Method::GET && path == "/" {
        let body = serde_json::json!({
            "apiVersion": ACP_API_VERSION,
            "transport": "toon-rpc",
            "endpoints": ["/agents", "/agents/{name}/runs", "/runs/{id}"]
        });
        return Ok(json_or_toon(&body, accept_toon, StatusCode::OK));
    }

    Ok(json_or_toon(
        &serde_json::json!({"error": "not found"}),
        accept_toon,
        StatusCode::NOT_FOUND,
    ))
}

fn wants_toon(req: &Request<Incoming>) -> bool {
    req.headers()
        .get("Accept")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("application/toon"))
        .unwrap_or(false)
}

fn handle_list_agents<S: AcpService>(service: &S, toon: bool) -> Response<String> {
    let agents: Vec<AgentSummary> = service
        .list_agents()
        .into_iter()
        .map(|a| AgentSummary {
            name: a.name,
            description: a.description,
            version: a.version,
        })
        .collect();
    json_or_toon(&serde_json::json!(agents), toon, StatusCode::OK)
}

async fn handle_create_run<S: AcpService>(
    req: Request<Incoming>,
    service: &S,
    runs: RunsMap,
    agent_name: &str,
    toon: bool,
) -> Response<String> {
    let body = match req.into_body().collect().await {
        Ok(b) => b.to_bytes().to_vec(),
        Err(_) => {
            return json_or_toon(
                &serde_json::json!({"error": "body error"}),
                toon,
                StatusCode::BAD_REQUEST,
            );
        }
    };

    let input: AgentRunInput = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return json_or_toon(
                &serde_json::json!({"error": format!("invalid body: {}", e)}),
                toon,
                StatusCode::BAD_REQUEST,
            );
        }
    };

    if service.get_agent(agent_name).is_none() {
        return json_or_toon(
            &serde_json::json!({"error": format!("agent not found: {}", agent_name)}),
            toon,
            StatusCode::NOT_FOUND,
        );
    }

    let mut run = service.run(agent_name, input.parts);
    let run_id = uuid::Uuid::new_v4().to_string();
    run.agent_run_id = run_id.clone();
    runs.lock().insert(run_id.clone(), run.clone());

    json_or_toon(&serde_json::to_value(&run).unwrap(), toon, StatusCode::OK)
}

fn handle_get_run(runs: &RunsMap, run_id: &str, toon: bool) -> Response<String> {
    match runs.lock().get(run_id) {
        Some(run) => json_or_toon(&serde_json::to_value(run).unwrap(), toon, StatusCode::OK),
        None => json_or_toon(
            &serde_json::json!({"error": format!("run not found: {}", run_id)}),
            toon,
            StatusCode::NOT_FOUND,
        ),
    }
}

fn handle_cancel_run<S: AcpService>(
    service: &S,
    runs: &RunsMap,
    run_id: &str,
    toon: bool,
) -> Response<String> {
    if !runs.lock().contains_key(run_id) {
        return json_or_toon(
            &serde_json::json!({"error": format!("run not found: {}", run_id)}),
            toon,
            StatusCode::NOT_FOUND,
        );
    }

    match service.cancel(run_id) {
        Ok(()) => {
            runs.lock().remove(run_id);
            json_or_toon(
                &serde_json::json!({"status": "cancelled", "runId": run_id}),
                toon,
                StatusCode::OK,
            )
        }
        Err(e) => json_or_toon(
            &serde_json::json!({"error": e.to_string()}),
            toon,
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    }
}

fn json_or_toon(value: &serde_json::Value, toon: bool, status: StatusCode) -> Response<String> {
    if toon {
        let toon_value = ToonValue::from_json_value(value.clone());
        match reddb_io_toon::encode(&toon_value) {
            Ok(s) => Response::builder()
                .status(status)
                .header("Content-Type", "application/toon")
                .body(s)
                .unwrap(),
            Err(_) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("toon encode error".to_string())
                .unwrap(),
        }
    } else {
        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(value.to_string())
            .unwrap()
    }
}
