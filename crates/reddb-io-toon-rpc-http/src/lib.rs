use http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use reddb_io_toon_rpc::Dispatcher;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct HttpService {
    dispatcher: Dispatcher,
}

impl HttpService {
    pub fn new(dispatcher: Dispatcher) -> Self {
        Self { dispatcher }
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr: SocketAddr = "0.0.0.0:8080".parse()?;
        let listener = TcpListener::bind(addr).await?;
        println!("TOON-RPC HTTP server listening on http://{}", addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let service = self.clone();

            tokio::spawn(async move {
                let connection = hyper_util::rt::TokioIo::new(stream);

                let hyper_service = hyper::service::service_fn(move |req| {
                    let svc = service.clone();
                    async move { hyper::service::Service::call(&svc, req).await }
                });

                if let Err(e) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(connection, hyper_service)
                    .await
                {
                    eprintln!("Error serving connection: {}", e);
                }
            });
        }
    }
}

impl hyper::service::Service<Request<Incoming>> for HttpService {
    type Response = Response<String>;
    type Error = Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let dispatcher = self.dispatcher.clone();

        Box::pin(async move {
            let res = Response::builder();

            if req.method() != http::Method::POST {
                return Ok(res
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body("Method not allowed".to_string())
                    .unwrap());
            }

            let body_result = req.into_body().collect().await;
            let body = match body_result {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let error_response = serde_json::json!({
                        "toonrpc": "1.0",
                        "error": {
                            "code": -32700,
                            "message": format!("Body error: {}", e)
                        },
                        "id": null
                    });
                    return Ok(res
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/toon")
                        .body(error_response.to_string())
                        .unwrap());
                }
            };

            let body_vec = body.to_vec();

            match dispatcher.dispatch(&body_vec) {
                Ok(response) => {
                    let text = String::from_utf8(response).unwrap();
                    Ok(res
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/toon")
                        .body(text)
                        .unwrap())
                }
                Err(e) => {
                    let error_response = serde_json::json!({
                        "toonrpc": "1.0",
                        "error": {
                            "code": -32603,
                            "message": e.to_string()
                        },
                        "id": null
                    });
                    Ok(res
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/toon")
                        .body(error_response.to_string())
                        .unwrap())
                }
            }
        })
    }
}
