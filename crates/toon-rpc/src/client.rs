use crate::error::RpcError;
use crate::protocol::{Request, TOONRPC_VERSION};
use crate::types::{Id, Params, Value};

pub struct Client<T> {
    transport: T,
    next_id: u64,
}

impl<T> Client<T>
where
    T: ClientTransport,
{
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            next_id: 1,
        }
    }

    pub async fn call(&mut self, method: &str, params: Params) -> Result<Value, RpcError> {
        let id = Id::Number(self.next_id as i64);
        self.next_id += 1;

        let request = Request {
            toonrpc: TOONRPC_VERSION.to_string(),
            method: method.to_string(),
            params,
            id: id.clone(),
        };

        let msg = crate::protocol::Message::Single(crate::protocol::Call::Request(request));
        let wire = crate::to_wire(&msg)?;

        self.transport.send(wire).await?;

        let response_bytes = self.transport.recv().await?;
        let response_msg = crate::from_wire(&response_bytes)?;

        match response_msg {
            crate::protocol::Message::SingleResponse(response) => {
                match (response.result, response.error) {
                    (Some(result), None) => Ok(result),
                    (None, Some(error)) => Err(error.into()),
                    _ => Err(RpcError::InvalidRequest("Invalid response".into())),
                }
            }
            _ => Err(RpcError::InvalidRequest("Expected single response".into())),
        }
    }
}

#[async_trait::async_trait]
pub trait ClientTransport: Send + Sync {
    async fn send(&self, data: Vec<u8>) -> Result<(), RpcError>;
    async fn recv(&self) -> Result<Vec<u8>, RpcError>;
}
