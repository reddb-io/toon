enum ValueSlot {
    Object(Vec<Field>),
    Array(Vec<Value>),
}

struct EventValueBuilder {
    stack: Vec<(ValueSlot, Option<String>)>,
    root: Option<Value>,
    /// Whether a repeated key replaces the earlier field (non-strict,
    /// last-write-wins). Strict decoding rejects duplicates in the grammar, so
    /// it skips the per-key search, which is quadratic on wide objects.
    dedupe: bool,
}

impl EventValueBuilder {
    fn new() -> Self {
        Self::with_dedupe(true)
    }

    fn with_dedupe(dedupe: bool) -> Self {
        Self {
            stack: Vec::new(),
            root: None,
            dedupe,
        }
    }

    fn attach(&mut self, value: Value) {
        match self.stack.last_mut() {
            None => self.root = Some(value),
            Some((ValueSlot::Array(items), _)) => items.push(value),
            Some((ValueSlot::Object(fields), pending)) => {
                if let Some(key) = pending.take() {
                    if self.dedupe {
                        if let Some(existing) = fields.iter_mut().find(|field| field.key == key) {
                            existing.value = value;
                            return;
                        }
                    }
                    fields.push(Field { key, value });
                }
            }
        }
    }

    fn push(&mut self, event: ToonEvent) {
        match event {
            ToonEvent::StartObject { .. } => {
                self.stack.push((ValueSlot::Object(Vec::new()), None));
            }
            ToonEvent::StartArray { .. } => {
                self.stack.push((ValueSlot::Array(Vec::new()), None));
            }
            ToonEvent::EndObject { .. } | ToonEvent::EndArray { .. } => {
                if let Some((slot, _)) = self.stack.pop() {
                    let value = match slot {
                        ValueSlot::Object(fields) => Value::Object(Document { fields }),
                        ValueSlot::Array(items) => Value::Array(Array::List(items)),
                    };
                    self.attach(value);
                }
            }
            ToonEvent::Key { key, .. } => {
                if let Some((_, pending)) = self.stack.last_mut() {
                    *pending = Some(key);
                }
            }
            ToonEvent::Primitive { value, .. } => self.attach(value),
        }
    }

    fn finish(self) -> Value {
        self.root.unwrap_or(Value::Object(Document { fields: Vec::new() }))
    }
}

/// The grammar can emit straight into the builder, so a whole-document decode
/// never materializes its event sequence.
impl EventSink for EventValueBuilder {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError> {
        self.push(event);
        Ok(())
    }
}

pub fn build_value_from_events(events: &[ToonEvent]) -> Value {
    let mut builder = EventValueBuilder::new();
    for event in events {
        builder.push(event.clone());
    }
    builder.finish()
}
