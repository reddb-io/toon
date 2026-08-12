//! The `toon` CLI error boundary, mirroring the upstream `@toon-format/cli`
//! presentation: a condition the CLI recognized and phrased for a human prints
//! one clean line, and a positioned decode failure prints the offending source
//! line with a caret under it. `--verbose` adds the cause chain.
//!
//! The TypeScript twins are `packages/toon/src/cli/errors.ts` and
//! `format-error.ts`. Rust has no stack to append, so `--verbose` carries the
//! cause chain alone.

use std::fmt;

/// A condition the CLI recognized and phrased for a human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    message: String,
    cause: Option<String>,
}

impl CliError {
    /// Raises a recognized failure with no underlying cause to report.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cause: None,
        }
    }

    /// Raises a recognized failure that wraps the error it tripped over.
    pub fn with_cause(message: impl Into<String>, cause: impl fmt::Display) -> Self {
        Self {
            message: message.into(),
            cause: Some(cause.to_string()),
        }
    }

    /// Renders a decode failure the way upstream does: a header, the offending
    /// source line, and a caret under the first character that could have
    /// caused it. `source` is absent when the line has scrolled out of the
    /// bounded window the CLI keeps for reporting.
    pub fn decode(line: usize, reason: &str, source: Option<&str>) -> Self {
        let header = format!("Failed to decode TOON at line {line}: {reason}");
        let Some(source) = source else {
            return Self::new(header);
        };

        let visible = source.replace('\t', "→");
        let first_non_whitespace = visible
            .chars()
            .position(|character| !character.is_whitespace())
            .unwrap_or(0);
        let gutter = format!("  {line} | ");
        let caret_indent = " ".repeat(gutter.chars().count() + first_non_whitespace);

        Self::new(format!("{header}\n\n{gutter}{visible}\n{caret_indent}^"))
    }

    /// Builds the stderr body for a failed run, without the `✖ ` prefix.
    pub fn report(&self, verbose: bool) -> String {
        match (verbose, &self.cause) {
            (true, Some(cause)) => format!("{}\n\nCaused by: {cause}", self.message),
            _ => self.message.clone(),
        }
    }
}

/// The message, and only the message: the cause chain belongs to `--verbose`,
/// which asks for it explicitly through [`CliError::report`].
impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}
