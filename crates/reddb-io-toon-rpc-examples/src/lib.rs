//! A calculator served and called through the code `reddb-io-toon-rpc-gen`
//! generated from `crates/reddb-io-toon-rpc-codegen/examples/calculator.toonrpc`.
//! The binaries serve it over HTTP and stdio; `tests/examples.rs` runs them.

use std::sync::Arc;

use reddb_io_toon_rpc::{Dispatcher, RpcError, RpcResult};

// Generated code is committed exactly as generated; the codegen tests check
// that it still matches the IDL.
#[rustfmt::skip]
pub mod calculator_api;

use calculator_api::{Stats, Vec2};

/// The calculator the examples serve.
pub struct Calculator;

impl calculator_api::Calculator for Calculator {
    fn add(&self, a: f64, b: f64) -> RpcResult<f64> {
        Ok(a + b)
    }

    fn divide(&self, a: f64, b: f64) -> RpcResult<f64> {
        if b == 0.0 {
            return Err(RpcError::InvalidParams("division by zero".into()));
        }
        Ok(a / b)
    }

    fn norm(&self, v: Vec2) -> RpcResult<f64> {
        Ok(v.x.hypot(v.y))
    }

    fn echo(&self, text: String) -> RpcResult<String> {
        Ok(text)
    }

    fn stats(&self, values: Vec<f64>) -> RpcResult<Stats> {
        let count = u32::try_from(values.len())
            .map_err(|_| RpcError::InvalidParams("too many values".into()))?;
        let mean = (count > 0).then(|| values.iter().sum::<f64>() / f64::from(count));
        Ok(Stats { count, mean })
    }
}

/// A dispatcher answering every Calculator method.
pub fn calculator_dispatcher() -> Dispatcher {
    let mut dispatcher = Dispatcher::new();
    calculator_api::register_calculator(&mut dispatcher, Arc::new(Calculator));
    dispatcher
}
