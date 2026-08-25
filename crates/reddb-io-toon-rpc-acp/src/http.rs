//! REST transport for the legacy ACP-style contract — agents and runs over HTTP.
//!
//! The wire shapes served here are pinned by `docs/acp-legacy-openapi.yaml`.
//! They are this repository's own legacy contract, not IBM/BeeAI's Agent
//! Communication Protocol and not Zed's Agent Client Protocol.
//!
//! - `GET    /agents`              — list all agents
//! - `POST   /agents/{name}/runs`  — start a new run
//! - `GET    /runs/{id}`           — fetch a retained run by id
//! - `DELETE /runs/{id}`           — cancel a live run, or release a finished one
//!
//! Responses are JSON by default. Clients that send `Accept: application/toon`
//! get TOON-encoded responses (the wire format that powers toon-rpc).

use crate::runs::{is_terminal, RunStore, DEFAULT_MAX_RUNS};
use crate::types::{AgentRunInput, AgentSummary, ACP_API_VERSION};
use crate::AcpService;
use http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use parking_lot::Mutex;
use reddb_io_toon::Value as ToonValue;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

type RunsMap = Arc<Mutex<RunStore>>;

/// Server-side limits for the legacy ACP HTTP surface.
#[derive(Debug, Clone)]
pub struct AcpHttpConfig {
    /// Maximum number of runs retained for later `GET /runs/{id}` reads.
    pub max_runs: usize,
}

impl Default for AcpHttpConfig {
    fn default() -> Self {
        Self {
            max_runs: DEFAULT_MAX_RUNS,
        }
    }
}

/// Run the ACP server over HTTP until the process is killed.
pub async fn serve_http<S: AcpService>(
    service: S,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr).await?;
    println!("[toon-rpc-acp] HTTP server listening on http://{}", addr);
    println!(
        "[toon-rpc-acp] try: curl -H 'Accept: application/toon' http://{}/agents",
        addr
    );
    serve_listener(service, listener, AcpHttpConfig::default()).await
}

/// Serve the ACP HTTP surface on an already-bound listener.
///
/// Tests and embedders use this to bind an ephemeral port and learn its
/// address before any request is made.
pub async fn serve_listener<S: AcpService>(
    service: S,
    listener: TcpListener,
    config: AcpHttpConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = Arc::new(service);
    let runs: RunsMap = Arc::new(Mutex::new(RunStore::new(config.max_runs)));

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
        return Ok(handle_create_run(req, service, runs, agent_name, accept_toon).await);
    }

    if method == http::Method::GET && path.starts_with("/runs/") {
        let run_id = path.trim_start_matches("/runs/").to_string();
        return Ok(handle_get_run(&runs, &run_id, accept_toon));
    }

    if method == http::Method::DELETE && path.starts_with("/runs/") {
        let run_id = path.trim_start_matches("/runs/").to_string();
        return Ok(handle_cancel_run(service, runs, run_id, accept_toon).await);
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
    service: Arc<S>,
    runs: RunsMap,
    agent_name: String,
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

    if service.get_agent(&agent_name).is_none() {
        return json_or_toon(
            &serde_json::json!({"error": format!("agent not found: {}", agent_name)}),
            toon,
            StatusCode::NOT_FOUND,
        );
    }

    // `AcpService::run` is a synchronous, caller-defined agent body: it may
    // block for the whole length of an agent run. Running it inline would pin
    // a tokio worker for that duration and stall every other connection, so it
    // goes to the blocking pool. The trait stays synchronous on purpose — this
    // contract is terminal, and an async trait method would break every
    // existing implementor for no wire-visible gain.
    let blocking_service = service.clone();
    let blocking_name = agent_name.clone();
    let parts = input.parts.clone();
    let run =
        tokio::task::spawn_blocking(move || blocking_service.run(&blocking_name, parts)).await;

    let mut run = match run {
        Ok(run) => run,
        Err(e) => {
            return json_or_toon(
                &serde_json::json!({"error": format!("agent run panicked: {}", e)}),
                toon,
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    let run_id = uuid::Uuid::new_v4().to_string();
    run.agent_run_id = run_id.clone();
    runs.lock().insert(run_id, run.clone());

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

async fn handle_cancel_run<S: AcpService>(
    service: Arc<S>,
    runs: RunsMap,
    run_id: String,
    toon: bool,
) -> Response<String> {
    let terminal = runs.lock().get(&run_id).map(|run| is_terminal(&run.status));

    let Some(terminal) = terminal else {
        return json_or_toon(
            &serde_json::json!({"error": format!("run not found: {}", run_id)}),
            toon,
            StatusCode::NOT_FOUND,
        );
    };

    // A finished run has nothing to cancel: releasing it is bookkeeping, and
    // must not be routed through a `cancel` hook that is allowed to fail.
    if terminal {
        let status = runs.lock().remove(&run_id).map(|run| run.status);
        return json_or_toon(
            &serde_json::json!({"status": status, "runId": run_id}),
            toon,
            StatusCode::OK,
        );
    }

    let cancel_id = run_id.clone();
    let cancelled = tokio::task::spawn_blocking(move || service.cancel(&cancel_id)).await;

    match cancelled {
        Ok(Ok(())) => {
            runs.lock().remove(&run_id);
            json_or_toon(
                &serde_json::json!({"status": "cancelled", "runId": run_id}),
                toon,
                StatusCode::OK,
            )
        }
        Ok(Err(e)) => json_or_toon(
            &serde_json::json!({"error": e.to_string()}),
            toon,
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        Err(e) => json_or_toon(
            &serde_json::json!({"error": format!("cancel panicked: {}", e)}),
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
