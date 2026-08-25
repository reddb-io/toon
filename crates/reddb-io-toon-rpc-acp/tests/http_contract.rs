//! Integration tests for the pinned legacy ACP-style REST contract.
//!
//! Shapes asserted here are the ones frozen in `docs/acp-legacy-openapi.yaml`.

use reddb_io_toon_rpc_acp::{
    serve_listener, AcpError, AcpHttpConfig, AcpResult, AcpService, Agent, AgentMessage, AgentRun,
    MessagePart,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// An agent whose body blocks the calling thread, like a real agent run does.
struct SlowEcho {
    delay: Duration,
    started: Arc<AtomicUsize>,
}

impl AcpService for SlowEcho {
    fn list_agents(&self) -> Vec<Agent> {
        vec![Agent {
            name: "echo".into(),
            description: "Echoes the user's message back.".into(),
            version: Some("0.1.0".into()),
            metadata: None,
        }]
    }

    fn run(&self, agent: &str, input_parts: Vec<MessagePart>) -> AgentRun {
        self.started.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(self.delay);
        let text = input_parts
            .iter()
            .filter_map(|p| p.content.as_ref().and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        AgentRun::completed(
            "placeholder",
            agent,
            vec![AgentMessage {
                role: "assistant".into(),
                parts: vec![MessagePart::text(text)],
                metadata: None,
            }],
        )
    }
}

/// An agent that stays live, so cancel is actually consulted.
struct LiveAgent {
    cancel_result: fn() -> AcpResult<()>,
}

impl AcpService for LiveAgent {
    fn list_agents(&self) -> Vec<Agent> {
        vec![Agent {
            name: "live".into(),
            description: "Never finishes.".into(),
            version: None,
            metadata: None,
        }]
    }

    fn run(&self, agent: &str, _input_parts: Vec<MessagePart>) -> AgentRun {
        let mut run = AgentRun::completed("placeholder", agent, vec![]);
        run.status = reddb_io_toon_rpc_acp::RunStatus::InProgress;
        run
    }

    fn cancel(&self, _run_id: &str) -> AcpResult<()> {
        (self.cancel_result)()
    }
}

struct HttpResponse {
    status: u16,
    body: String,
}

async fn request(addr: std::net::SocketAddr, raw: String) -> HttpResponse {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(raw.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read");
    let text = String::from_utf8(buf).expect("utf8");
    let (head, body) = text.split_once("\r\n\r\n").expect("headers");
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("status");
    HttpResponse {
        status,
        body: body.to_string(),
    }
}

async fn get(addr: std::net::SocketAddr, path: &str, accept: &str) -> HttpResponse {
    request(
        addr,
        format!(
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nAccept: {accept}\r\nConnection: close\r\n\r\n"
        ),
    )
    .await
}

async fn delete(addr: std::net::SocketAddr, path: &str) -> HttpResponse {
    request(
        addr,
        format!(
            "DELETE {path} HTTP/1.1\r\nHost: localhost\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        ),
    )
    .await
}

async fn post(addr: std::net::SocketAddr, path: &str, body: &str) -> HttpResponse {
    request(
        addr,
        format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
            len = body.len()
        ),
    )
    .await
}

async fn spawn_server<S: AcpService>(service: S, config: AcpHttpConfig) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        let _ = serve_listener(service, listener, config).await;
    });
    addr
}

const ECHO_BODY: &str =
    r#"{"parts":[{"kind":"text","content_type":"text/plain","content":"hello","status":"done"}]}"#;

#[tokio::test]
async fn create_run_get_run_and_release_completed_run() {
    let addr = spawn_server(
        SlowEcho {
            delay: Duration::from_millis(0),
            started: Arc::new(AtomicUsize::new(0)),
        },
        AcpHttpConfig::default(),
    )
    .await;

    let agents = get(addr, "/agents", "application/json").await;
    assert_eq!(agents.status, 200);
    let listed: serde_json::Value = serde_json::from_str(&agents.body).expect("json");
    assert_eq!(listed[0]["name"], "echo");

    let created = post(addr, "/agents/echo/runs", ECHO_BODY).await;
    assert_eq!(created.status, 200);
    let run: serde_json::Value = serde_json::from_str(&created.body).expect("json");
    assert_eq!(run["agentName"], "echo");
    assert_eq!(run["status"], "completed");
    assert_eq!(run["output"][0]["parts"][0]["content"], "hello");
    let run_id = run["agentRunId"].as_str().expect("run id").to_string();
    assert_ne!(run_id, "placeholder");

    let fetched = get(addr, &format!("/runs/{run_id}"), "application/json").await;
    assert_eq!(fetched.status, 200);
    let fetched_run: serde_json::Value = serde_json::from_str(&fetched.body).expect("json");
    assert_eq!(fetched_run["agentRunId"], run_id.as_str());

    // Reading must not consume the run.
    let again = get(addr, &format!("/runs/{run_id}"), "application/json").await;
    assert_eq!(again.status, 200);

    // A completed run is released without consulting the failing default
    // `cancel` hook, so it must not answer 500.
    let released = delete(addr, &format!("/runs/{run_id}")).await;
    assert_eq!(released.status, 200);
    let ack: serde_json::Value = serde_json::from_str(&released.body).expect("json");
    assert_eq!(ack["status"], "completed");
    assert_eq!(ack["runId"], run_id.as_str());

    let gone = get(addr, &format!("/runs/{run_id}"), "application/json").await;
    assert_eq!(gone.status, 404);
    let gone_delete = delete(addr, &format!("/runs/{run_id}")).await;
    assert_eq!(gone_delete.status, 404);
}

#[tokio::test]
async fn toon_accept_header_switches_the_response_encoding() {
    let addr = spawn_server(
        SlowEcho {
            delay: Duration::from_millis(0),
            started: Arc::new(AtomicUsize::new(0)),
        },
        AcpHttpConfig::default(),
    )
    .await;

    let created = post(addr, "/agents/echo/runs", ECHO_BODY).await;
    assert_eq!(created.status, 200);

    let agents = get(addr, "/agents", "application/toon").await;
    assert_eq!(agents.status, 200);
    assert!(
        agents.body.contains("{name,description,version}"),
        "expected a TOON table header, got {}",
        agents.body
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&agents.body).is_err(),
        "TOON responses must not be JSON: {}",
        agents.body
    );
}

#[tokio::test]
async fn unknown_agents_runs_and_routes_answer_404() {
    let addr = spawn_server(
        SlowEcho {
            delay: Duration::from_millis(0),
            started: Arc::new(AtomicUsize::new(0)),
        },
        AcpHttpConfig::default(),
    )
    .await;

    assert_eq!(post(addr, "/agents/nope/runs", ECHO_BODY).await.status, 404);
    assert_eq!(
        get(addr, "/runs/does-not-exist", "application/json")
            .await
            .status,
        404
    );
    assert_eq!(get(addr, "/nope", "application/json").await.status, 404);
    assert_eq!(
        post(addr, "/agents/echo/runs", "not json").await.status,
        400
    );

    let descriptor = get(addr, "/", "application/json").await;
    assert_eq!(descriptor.status, 200);
    let value: serde_json::Value = serde_json::from_str(&descriptor.body).expect("json");
    assert_eq!(value["apiVersion"], "0.1.0");
}

#[tokio::test]
async fn cancelling_a_live_run_consults_the_service() {
    let addr = spawn_server(
        LiveAgent {
            cancel_result: || Ok(()),
        },
        AcpHttpConfig::default(),
    )
    .await;
    let created = post(addr, "/agents/live/runs", ECHO_BODY).await;
    let run: serde_json::Value = serde_json::from_str(&created.body).expect("json");
    let run_id = run["agentRunId"].as_str().expect("run id").to_string();

    let cancelled = delete(addr, &format!("/runs/{run_id}")).await;
    assert_eq!(cancelled.status, 200);
    let ack: serde_json::Value = serde_json::from_str(&cancelled.body).expect("json");
    assert_eq!(ack["status"], "cancelled");
    assert_eq!(
        get(addr, &format!("/runs/{run_id}"), "application/json")
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn a_refused_cancel_of_a_live_run_keeps_the_run() {
    let addr = spawn_server(
        LiveAgent {
            cancel_result: || Err(AcpError::Internal("cancel not supported".into())),
        },
        AcpHttpConfig::default(),
    )
    .await;
    let created = post(addr, "/agents/live/runs", ECHO_BODY).await;
    let run: serde_json::Value = serde_json::from_str(&created.body).expect("json");
    let run_id = run["agentRunId"].as_str().expect("run id").to_string();

    let refused = delete(addr, &format!("/runs/{run_id}")).await;
    assert_eq!(refused.status, 500);
    assert_eq!(
        get(addr, &format!("/runs/{run_id}"), "application/json")
            .await
            .status,
        200
    );
}

#[tokio::test]
async fn run_retention_is_bounded() {
    let addr = spawn_server(
        SlowEcho {
            delay: Duration::from_millis(0),
            started: Arc::new(AtomicUsize::new(0)),
        },
        AcpHttpConfig { max_runs: 2 },
    )
    .await;

    let mut ids = Vec::new();
    for _ in 0..4 {
        let created = post(addr, "/agents/echo/runs", ECHO_BODY).await;
        let run: serde_json::Value = serde_json::from_str(&created.body).expect("json");
        ids.push(run["agentRunId"].as_str().expect("run id").to_string());
    }

    let mut retained = 0;
    for id in &ids {
        if get(addr, &format!("/runs/{id}"), "application/json")
            .await
            .status
            == 200
        {
            retained += 1;
        }
    }
    assert_eq!(retained, 2, "retention must be bounded by max_runs");
}

/// The whole point of running the agent body on the blocking pool: on a
/// single-threaded runtime, a run in flight must not stall other requests.
#[test]
fn a_blocking_agent_run_does_not_stall_the_async_runtime() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    runtime.block_on(async {
        let started = Arc::new(AtomicUsize::new(0));
        let addr = spawn_server(
            SlowEcho {
                delay: Duration::from_millis(1500),
                started: started.clone(),
            },
            AcpHttpConfig::default(),
        )
        .await;

        let slow = tokio::spawn(async move { post(addr, "/agents/echo/runs", ECHO_BODY).await });

        // Wait until the agent body is actually executing.
        while started.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let began = Instant::now();
        let agents = get(addr, "/agents", "application/json").await;
        let elapsed = began.elapsed();
        assert_eq!(agents.status, 200);
        assert!(
            elapsed < Duration::from_millis(1000),
            "an unrelated request waited {:?} behind a blocking agent run",
            elapsed
        );

        let created = slow.await.expect("join");
        assert_eq!(created.status, 200);
    });
}
