//! The documents a query can still read with `input` and `inputs`.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use reddb_io_toon::Value;

/// A handle on the reader the CLI is feeding the query from, shared rather
/// than copied: the row loop and the `input`/`inputs` builtins pull from the
/// same cursor, so a row a filter consumes is one the loop no longer sees and
/// a stream is never slurped into memory to be read twice.
#[derive(Clone)]
pub(crate) struct Inputs(Rc<RefCell<dyn Iterator<Item = Result<Value, String>>>>);

impl fmt::Debug for Inputs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Inputs")
    }
}

impl Inputs {
    pub(crate) fn new(source: impl Iterator<Item = Result<Value, String>> + 'static) -> Self {
        Self(Rc::new(RefCell::new(source)))
    }

    /// The next document, or `None` once the reader is exhausted.
    pub(crate) fn next_input(&self) -> Option<Result<Value, String>> {
        self.0.borrow_mut().next()
    }
}
