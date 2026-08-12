//! `tq jq-check`: the machine-readable jq-compatibility decision.
//!
//! One JSON object on stdout, exit `0` when the invocation is compatible and
//! `1` when it is not. The filter is never evaluated, so a negative decision
//! costs no partial interpretation and no input is read.

use std::process::ExitCode;

use crate::query::compat::{self, Reason};

use super::args::JqCheckOptions;

/// The jq release the parity corpus pins, and the only one a positive decision
/// speaks for.
const JQ_VERSION: &str = "1.7.1";

pub(super) fn run_jq_check(options: JqCheckOptions) -> Result<(String, ExitCode), String> {
    let mut reasons = options
        .rejected
        .iter()
        .map(|detail| Reason::new(compat::UNSUPPORTED_OPTION, detail.clone()))
        .collect::<Vec<_>>();
    reasons.extend(compat::classify(&options.filter));

    let compatible = reasons.is_empty();
    let report = serde_json::json!({
        "jq_version": JQ_VERSION,
        "filter": options.filter,
        "options": options.options,
        "compatible": compatible,
        "reasons": reasons
            .iter()
            .map(|reason| serde_json::json!({"kind": reason.kind, "detail": reason.detail}))
            .collect::<Vec<_>>(),
    });

    let output = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    let code = if compatible {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    };
    Ok((format!("{output}\n"), code))
}
