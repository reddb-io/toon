//! `halt` and `halt_error` end the whole run rather than one evaluation.
//!
//! Evaluation errors are strings throughout tq, so a halt travels as one too:
//! it unwinds the query the same way an error does, but carries a prefix no
//! ordinary message can produce. `try`, `?` and `//` re-raise anything wearing
//! that prefix instead of recovering from it, and the CLI turns it back into
//! the stderr text and exit status jq would have produced.

const HALT_PREFIX: &str = "\u{1e}tq:halt:";

/// The exit status a halted query asked for, and the bytes it asked to be
/// written to stderr first.
pub(crate) struct Halt {
    pub(crate) code: u8,
    pub(crate) message: String,
}

impl Halt {
    /// The error string that carries a halt up through the evaluator.
    pub(super) fn raise(code: u8, message: String) -> String {
        format!("{HALT_PREFIX}{code}:{message}")
    }

    /// The halt an evaluation error carries, or `None` for an ordinary error.
    pub(crate) fn decode(error: &str) -> Option<Self> {
        let (code, message) = error.strip_prefix(HALT_PREFIX)?.split_once(':')?;
        Some(Self {
            code: code.parse().ok()?,
            message: message.to_owned(),
        })
    }
}

pub(super) fn is_halt(error: &str) -> bool {
    error.starts_with(HALT_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A raised halt survives the round trip, and nothing else is mistaken for
    /// one: neither an ordinary message nor a prefix without a whole status.
    #[test]
    fn only_a_well_formed_halt_decodes_as_one() {
        let raised = Halt::raise(3, "stop".to_owned());
        let halt = Halt::decode(&raised).expect("a raised halt decodes");

        assert_eq!(halt.code, 3);
        assert_eq!(halt.message, "stop");
        assert!(is_halt(&raised));

        assert!(Halt::decode("No more inputs").is_none());
        assert!(Halt::decode(HALT_PREFIX).is_none());
        assert!(Halt::decode(&format!("{HALT_PREFIX}notastatus:stop")).is_none());
    }
}
