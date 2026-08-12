use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use reddb_io_toon::{Array, Value};

use super::assign;
use super::ast::{BinaryOp, Expr, Pattern};
use super::builtins;
use super::halt;
use super::inputs::Inputs;
use super::ordering::{compare_json_values, compare_key_json};

// Evaluation errors remain strings throughout tq. `error(v)` reserves this
// prefix for non-string jq payloads; only recovery filters decode it, so a
// caught object or array can round-trip without widening every builtin's
// Result type. The suffix is compact JSON produced by serde_json.
const TAGGED_ERROR_PREFIX: &str = "\u{1e}tq:error:json:";

// jq recurses on its own interpreter stack without a depth limit, while tq
// evaluates a nested filter as a nested Rust call. A runaway definition
// therefore has to be reported as an ordinary diagnostic well before it can
// exhaust the process stack. The budget counts evaluation nesting rather than
// user function calls alone, because that is the resource actually at stake:
// one call level spends several nested evaluations.
const MAX_EVAL_DEPTH: usize = 500;

const DEPTH_ERROR: &str = "exceeded the maximum filter recursion depth of 500";

#[derive(Debug, Clone)]
pub(super) struct Env {
    frames: Vec<HashMap<String, Value>>,
    functions: Vec<Rc<Function>>,
    environment: Value,
    /// Shared by every environment derived from one query, so nesting is
    /// counted across bindings, definitions and calls alike.
    budget: Rc<Budget>,
    /// The documents still to be read, shared with whatever feeds the query so
    /// `input`/`inputs` draw from the live reader rather than a copy. `None`
    /// where the input is a single document, which is also where jq's `input`
    /// has nothing left to hand back.
    inputs: Option<Inputs>,
}

/// The evaluation nesting budget for one query.
#[derive(Debug, Default)]
struct Budget {
    depth: Cell<usize>,
    /// Exhausting the budget ends the whole query. Reporting a recoverable
    /// error instead would let `try`, `?` and `//` retry the runaway body at
    /// every level it unwound through, turning a bomb into exponential work.
    exhausted: Cell<bool>,
}

/// Releases one unit of the nesting budget, on the error paths too.
pub(super) struct Depth(Rc<Budget>);

impl Drop for Depth {
    fn drop(&mut self) {
        self.0.depth.set(self.0.depth.get().saturating_sub(1));
    }
}

/// A user-defined filter: the parameter names it declares, its body, and the
/// environment it closed over where it was defined.
#[derive(Debug)]
struct Function {
    name: String,
    parameters: Vec<String>,
    body: Rc<Expr>,
    environment: Env,
    /// A definition is visible inside its own body so it can recurse. An
    /// argument closure is not: in `def f(g): g; f(g)` the inner `g` has to
    /// stay the caller's `g` instead of shadowing itself.
    recursive: bool,
}

impl Default for Env {
    fn default() -> Self {
        let values = std::env::vars_os()
            .filter_map(|(key, value)| {
                Some((
                    key.into_string().ok()?,
                    serde_json::Value::String(value.into_string().ok()?),
                ))
            })
            .collect::<serde_json::Map<_, _>>();
        let environment = Value::from_json_value(serde_json::Value::Object(values));
        Self {
            frames: vec![HashMap::from([("ENV".to_owned(), environment.clone())])],
            functions: Vec::new(),
            environment,
            budget: Rc::new(Budget::default()),
            inputs: None,
        }
    }
}

impl Env {
    /// The base environment plus the variables named on the command line. They
    /// live in their own frame, so a query-level binding of the same name still
    /// shadows the flag inside its scope.
    pub(super) fn with_variables(variables: &[(String, Value)]) -> Self {
        let mut env = Self::default();
        env.frames.push(variables.iter().cloned().collect());
        env
    }

    /// Attaches the stream the query reads further documents from.
    pub(super) fn reading(mut self, inputs: &Inputs) -> Self {
        self.inputs = Some(inputs.clone());
        self
    }

    /// The next document of that stream, or `None` once it is exhausted or
    /// where there was never one to read.
    pub(super) fn next_input(&self) -> Option<Result<Value, String>> {
        self.inputs.as_ref()?.next_input()
    }

    pub(super) fn bind(&self, pattern: &Pattern, value: &Value) -> Result<Self, String> {
        let mut child = self.clone();
        let mut frame = HashMap::new();
        bind_pattern(pattern, value, &mut frame)?;
        child.frames.push(frame);
        Ok(child)
    }

    fn get(&self, name: &str) -> Option<&Value> {
        self.frames.iter().rev().find_map(|frame| frame.get(name))
    }

    pub(super) fn enter(&self) -> Result<Depth, String> {
        if self.budget.exhausted.get() {
            return Err(DEPTH_ERROR.to_owned());
        }
        let depth = self.budget.depth.get() + 1;
        if depth > MAX_EVAL_DEPTH {
            self.budget.exhausted.set(true);
            return Err(DEPTH_ERROR.to_owned());
        }
        self.budget.depth.set(depth);
        Ok(Depth(Rc::clone(&self.budget)))
    }

    pub(super) fn define(&self, name: &str, parameters: &[String], body: &Rc<Expr>) -> Self {
        let mut child = self.clone();
        child.functions.push(Rc::new(Function {
            name: name.to_owned(),
            parameters: parameters.to_vec(),
            body: Rc::clone(body),
            environment: self.clone(),
            recursive: true,
        }));
        child
    }

    /// The body of a user-defined filter and the environment it runs in, or
    /// `None` when the name belongs to a builtin instead. The latest definition
    /// wins, so a redefinition shadows an earlier one and a user function
    /// shadows the builtin of the same name and arity.
    pub(super) fn resolve_call(&self, name: &str, arguments: &[Expr]) -> Option<(Rc<Expr>, Self)> {
        let function = self.functions.iter().rev().find(|function| {
            function.name == name && function.parameters.len() == arguments.len()
        })?;
        Some((Rc::clone(&function.body), self.call(function, arguments)))
    }

    /// The environment a call body runs in: the definition site, the callee
    /// itself for recursion, and one closure per argument that keeps the
    /// caller's scope so a filter parameter still sees the caller's bindings.
    fn call(&self, function: &Rc<Function>, arguments: &[Expr]) -> Self {
        let mut child = function.environment.clone();
        if function.recursive {
            child.functions.push(Rc::clone(function));
        }
        for (name, argument) in function.parameters.iter().zip(arguments) {
            child.functions.push(Rc::new(Function {
                name: name.clone(),
                parameters: Vec::new(),
                body: Rc::new(argument.clone()),
                environment: self.clone(),
                recursive: false,
            }));
        }
        child
    }
}

impl Expr {
    pub(super) fn eval(&self, input: &Value, env: &Env) -> Result<Vec<Value>, String> {
        let _depth = env.enter()?;
        self.evaluate(input, env)
    }

    fn evaluate(&self, input: &Value, env: &Env) -> Result<Vec<Value>, String> {
        match self {
            Self::Alternative(left, right) => {
                let produced = match left.eval(input, env) {
                    Ok(values) => values,
                    // `//` suppresses the errors its left-hand side raises, but
                    // a halt is the end of the program rather than an error to
                    // recover from.
                    Err(error) if halt::is_halt(&error) => return Err(error),
                    Err(_) => Vec::new(),
                };
                let values = produced.into_iter().filter(is_truthy).collect::<Vec<_>>();
                if values.is_empty() {
                    right.eval(input, env)
                } else {
                    Ok(values)
                }
            }
            Self::Array(items) => {
                let mut values = Vec::new();
                for item in items {
                    values.extend(item.eval(input, env)?);
                }
                Ok(vec![Value::Array(Array::List(values))])
            }
            Self::Assign(operator, target, source) => {
                assign::evaluate(*operator, target, source, input, env)
            }
            Self::Bind(source, pattern, body) => {
                let mut output = Vec::new();
                for value in source.eval(input, env)? {
                    output.extend(body.eval(input, &env.bind(pattern, &value)?)?);
                }
                Ok(output)
            }
            Self::Binary(BinaryOp::And, left, right) => {
                let left_values = left.eval(input, env)?;
                if left_values.iter().all(|value| !is_truthy(value)) {
                    return Ok(left_values
                        .into_iter()
                        .map(|_| Value::Bool(false))
                        .collect());
                }
                let right_values = right.eval(input, env)?;
                let mut output = Vec::new();
                for left_value in left_values {
                    if is_truthy(&left_value) {
                        output.extend(
                            right_values
                                .iter()
                                .map(|right_value| Value::Bool(is_truthy(right_value))),
                        );
                    } else {
                        output.push(Value::Bool(false));
                    }
                }
                Ok(output)
            }
            Self::Binary(BinaryOp::Or, left, right) => {
                let left_values = left.eval(input, env)?;
                if left_values.iter().all(is_truthy) {
                    return Ok(left_values.into_iter().map(|_| Value::Bool(true)).collect());
                }
                let right_values = right.eval(input, env)?;
                let mut output = Vec::new();
                for left_value in left_values {
                    if is_truthy(&left_value) {
                        output.push(Value::Bool(true));
                    } else {
                        output.extend(
                            right_values
                                .iter()
                                .map(|right_value| Value::Bool(is_truthy(right_value))),
                        );
                    }
                }
                Ok(output)
            }
            Self::Binary(operator, left, right) => {
                let left_values = left.eval(input, env)?;
                let right_values = right.eval(input, env)?;
                let mut output = Vec::new();
                for left_value in &left_values {
                    for right_value in &right_values {
                        output.push(evaluate_binary(*operator, left_value, right_value)?);
                    }
                }
                Ok(output)
            }
            Self::Call(name, arguments) => match env.resolve_call(name, arguments) {
                Some((body, scope)) => body.eval(input, &scope),
                None => builtins::evaluate(name, arguments, input, env),
            },
            Self::Comma(expressions) => {
                let mut output = Vec::new();
                for expression in expressions {
                    output.extend(expression.eval(input, env)?);
                }
                Ok(output)
            }
            Self::Conditional(branches, fallback) => {
                evaluate_conditional(branches, fallback, input, env)
            }
            Self::Def {
                name,
                parameters,
                body,
                rest,
            } => rest.eval(input, &env.define(name, parameters, body)),
            Self::Empty => Ok(Vec::new()),
            Self::Environment => Ok(vec![env.environment.clone()]),
            Self::Field(base, key) => Ok(base
                .eval(input, env)?
                .into_iter()
                .map(|value| match value {
                    Value::Object(document) => document.get(key).cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
                .collect()),
            Self::Foreach {
                generator,
                pattern,
                initial,
                update,
                extract,
            } => {
                let generated = generator.eval(input, env)?;
                let mut states = initial.eval(input, env)?;
                let mut output = Vec::new();
                for value in generated {
                    let binding = env.bind(pattern, &value)?;
                    let mut next = Vec::new();
                    for state in states {
                        for updated in update.eval(&state, &binding)? {
                            output.extend(extract.eval(&updated, &binding)?);
                            next.push(updated);
                        }
                    }
                    states = next;
                }
                Ok(output)
            }
            Self::Identity => Ok(vec![input.clone()]),
            Self::Index(base, index) => super::indexing::evaluate_index(base, index, input, env),
            Self::Iter(base) => super::indexing::evaluate_iteration(base, input, env),
            Self::Literal(value) => Ok(vec![value.clone()]),
            Self::Object(fields) => evaluate_object(fields, input, env),
            Self::Optional(expression) => Ok(evaluate_recovering(expression, input, env).values),
            Self::Pipe(left, right) => {
                let mut output = Vec::new();
                for value in left.eval(input, env)? {
                    output.extend(right.eval(&value, env)?);
                }
                Ok(output)
            }
            Self::Reduce {
                generator,
                pattern,
                initial,
                update,
            } => {
                let generated = generator.eval(input, env)?;
                let mut states = initial.eval(input, env)?;
                for value in generated {
                    let binding = env.bind(pattern, &value)?;
                    let mut next = Vec::new();
                    for state in states {
                        next.extend(update.eval(&state, &binding)?);
                    }
                    states = next;
                }
                Ok(states)
            }
            Self::Slice(base, start, end) => {
                super::indexing::evaluate_slice(base, start.as_deref(), end.as_deref(), input, env)
            }
            Self::Try(expression, handler) => {
                recover_try(expression, handler.as_deref(), input, env).finish()
            }
            Self::Variable(name) => env
                .get(name)
                .cloned()
                .map(|value| vec![value])
                .ok_or_else(|| format!("variable `${name}` is not defined")),
        }
    }
}

fn bind_pattern(
    pattern: &Pattern,
    value: &Value,
    frame: &mut HashMap<String, Value>,
) -> Result<(), String> {
    match pattern {
        Pattern::Array(patterns) => {
            for (index, pattern) in patterns.iter().enumerate() {
                let element = match value {
                    Value::Array(values) => values.get(index).unwrap_or(Value::Null),
                    Value::Null => Value::Null,
                    value => {
                        return Err(format!("Cannot index {} with number", value_kind(value)));
                    }
                };
                bind_pattern(pattern, &element, frame)?;
            }
        }
        Pattern::Object(patterns) => {
            for (key, pattern) in patterns {
                let field = match value {
                    Value::Object(document) => document.get(key).cloned().unwrap_or(Value::Null),
                    Value::Null => Value::Null,
                    value => {
                        return Err(format!(
                            "Cannot index {} with string {}",
                            value_kind(value),
                            serde_json::to_string(key).expect("pattern key serializes")
                        ));
                    }
                };
                bind_pattern(pattern, &field, frame)?;
            }
        }
        Pattern::Variable(name) => {
            frame.insert(name.clone(), value.clone());
        }
    }
    Ok(())
}

struct Recovery {
    values: Vec<Value>,
    error: Option<String>,
}

impl Recovery {
    fn success(values: Vec<Value>) -> Self {
        Self {
            values,
            error: None,
        }
    }

    fn from_result(result: Result<Vec<Value>, String>) -> Self {
        match result {
            Ok(values) => Self::success(values),
            Err(error) => Self {
                values: Vec::new(),
                error: Some(error),
            },
        }
    }

    fn finish(self) -> Result<Vec<Value>, String> {
        self.error.map_or(Ok(self.values), Err)
    }
}

fn evaluate_recovering(expression: &Expr, input: &Value, env: &Env) -> Recovery {
    match expression {
        Expr::Comma(expressions) => {
            let mut recovery = Recovery::success(Vec::new());
            for expression in expressions {
                append_recovery(&mut recovery, evaluate_recovering(expression, input, env));
                if recovery.error.is_some() {
                    break;
                }
            }
            recovery
        }
        Expr::Conditional(branches, fallback) => {
            recover_conditional(branches, fallback, input, env)
        }
        Expr::Optional(expression) => {
            let mut recovery = evaluate_recovering(expression, input, env);
            if !recovery.error.as_deref().is_some_and(halt::is_halt) {
                recovery.error = None;
            }
            recovery
        }
        Expr::Pipe(left, right) => {
            let left = evaluate_recovering(left, input, env);
            let mut recovery = Recovery::success(Vec::new());
            for value in left.values {
                append_recovery(&mut recovery, evaluate_recovering(right, &value, env));
                if recovery.error.is_some() {
                    return recovery;
                }
            }
            recovery.error = left.error;
            recovery
        }
        Expr::Try(expression, handler) => recover_try(expression, handler.as_deref(), input, env),
        expression => Recovery::from_result(expression.eval(input, env)),
    }
}

fn recover_try(expression: &Expr, handler: Option<&Expr>, input: &Value, env: &Env) -> Recovery {
    let mut recovery = evaluate_recovering(expression, input, env);
    let Some(error) = recovery.error.take() else {
        return recovery;
    };
    // A halt passes straight through `try`, as it does in jq: there is no
    // program left for the handler to run in.
    if halt::is_halt(&error) {
        recovery.error = Some(error);
        return recovery;
    }
    if let Some(handler) = handler {
        append_recovery(
            &mut recovery,
            evaluate_recovering(handler, &decode_error_payload(&error), env),
        );
    }
    recovery
}

fn recover_conditional(
    branches: &[(Expr, Expr)],
    fallback: &Expr,
    input: &Value,
    env: &Env,
) -> Recovery {
    let Some(((condition, selected), remaining)) = branches.split_first() else {
        return evaluate_recovering(fallback, input, env);
    };
    let condition = evaluate_recovering(condition, input, env);
    let mut recovery = Recovery::success(Vec::new());
    for value in condition.values {
        let next = if is_truthy(&value) {
            evaluate_recovering(selected, input, env)
        } else {
            recover_conditional(remaining, fallback, input, env)
        };
        append_recovery(&mut recovery, next);
        if recovery.error.is_some() {
            return recovery;
        }
    }
    recovery.error = condition.error;
    recovery
}

fn append_recovery(target: &mut Recovery, next: Recovery) {
    target.values.extend(next.values);
    target.error = next.error;
}

fn evaluate_conditional(
    branches: &[(Expr, Expr)],
    fallback: &Expr,
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let Some(((condition, selected), remaining)) = branches.split_first() else {
        return fallback.eval(input, env);
    };

    let mut output = Vec::new();
    for value in condition.eval(input, env)? {
        if is_truthy(&value) {
            output.extend(selected.eval(input, env)?);
        } else {
            output.extend(evaluate_conditional(remaining, fallback, input, env)?);
        }
    }
    Ok(output)
}

fn encode_error_payload(value: &Value) -> String {
    match value {
        Value::String(message) => message.clone(),
        value => format!(
            "{TAGGED_ERROR_PREFIX}{}",
            serde_json::to_string(&value.to_json_value())
                .expect("tq values always serialize as JSON")
        ),
    }
}

fn decode_error_payload(error: &str) -> Value {
    error
        .strip_prefix(TAGGED_ERROR_PREFIX)
        .and_then(|json| serde_json::from_str(json).ok())
        .map(Value::from_json_value)
        .unwrap_or_else(|| Value::String(error.to_owned()))
}

fn evaluate_object(
    fields: &[(String, Expr)],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let mut objects = vec![serde_json::Map::new()];
    for (key, expression) in fields {
        let values = expression.eval(input, env)?;
        let mut next_objects = Vec::new();
        for object in objects {
            for value in &values {
                let mut next = object.clone();
                next.insert(key.clone(), value.to_json_value());
                next_objects.push(next);
            }
        }
        objects = next_objects;
    }

    Ok(objects
        .into_iter()
        .map(serde_json::Value::Object)
        .map(Value::from_json_value)
        .collect())
}

pub(super) fn call_add(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    evaluate_add(input).map(|value| vec![value])
}

pub(super) fn call_error(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let payload = if let Some(argument) = arguments.first() {
        let Some(value) = argument.eval(input, env)?.into_iter().next() else {
            return Ok(Vec::new());
        };
        value
    } else {
        input.clone()
    };
    Err(encode_error_payload(&payload))
}

pub(super) fn call_from_entries(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    evaluate_from_entries(input).map(|value| vec![value])
}

pub(super) fn call_group_by(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    evaluate_group_by(input, &arguments[0], env).map(|value| vec![value])
}

pub(super) fn call_has(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    arguments[0]
        .eval(input, env)?
        .into_iter()
        .map(|key| evaluate_has(input, &key))
        .collect()
}

pub(super) fn call_join(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    evaluate_join(
        input,
        &single_string_arg(&arguments[0], input, env, "join")?,
    )
    .map(|value| vec![value])
}

pub(super) fn call_keys(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    evaluate_keys(input).map(|value| vec![value])
}

pub(super) fn call_length(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    evaluate_length(input).map(|value| vec![value])
}

pub(super) fn call_map(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let Value::Array(array) = input else {
        return Err("cannot iterate over non-array".to_owned());
    };
    let mut values = Vec::new();
    for value in array.values() {
        values.extend(arguments[0].eval(&value, env)?);
    }
    Ok(vec![Value::Array(Array::List(values))])
}

pub(super) fn call_max_by(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    evaluate_min_max_by(input, &arguments[0], env, true).map(|value| vec![value])
}

pub(super) fn call_min_by(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    evaluate_min_max_by(input, &arguments[0], env, false).map(|value| vec![value])
}

pub(super) fn call_not(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    Ok(vec![Value::Bool(!is_truthy(input))])
}

pub(super) fn call_select(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    let mut values = Vec::new();
    for value in arguments[0].eval(input, env)? {
        if is_truthy(&value) {
            values.push(input.clone());
        }
    }
    Ok(values)
}

pub(super) fn call_sort_by(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    evaluate_sort_by(input, &arguments[0], env).map(|value| vec![value])
}

pub(super) fn call_split(
    arguments: &[Expr],
    input: &Value,
    env: &Env,
) -> Result<Vec<Value>, String> {
    evaluate_split(
        input,
        &single_string_arg(&arguments[0], input, env, "split")?,
    )
    .map(|value| vec![value])
}

pub(super) fn call_to_entries(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    evaluate_to_entries(input).map(|value| vec![value])
}

pub(super) fn call_unique(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    evaluate_unique(input).map(|value| vec![value])
}

fn evaluate_add(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("add cannot be applied to this value".to_owned());
    };

    let mut values = array.values().into_iter();
    let Some(mut total) = values.next() else {
        return Ok(Value::Null);
    };
    for value in values {
        total = add_values(&total, &value)?;
    }
    Ok(total)
}

fn evaluate_sort_by(input: &Value, filter: &Expr, env: &Env) -> Result<Value, String> {
    let mut keyed = keyed_array_values(input, filter, env)?;
    keyed.sort_by(|left, right| compare_key_json(&left.0, &right.0));
    Ok(Value::Array(Array::List(
        keyed.into_iter().map(|(_, value)| value).collect(),
    )))
}

fn evaluate_group_by(input: &Value, filter: &Expr, env: &Env) -> Result<Value, String> {
    let mut keyed = keyed_array_values(input, filter, env)?;
    keyed.sort_by(|left, right| compare_key_json(&left.0, &right.0));

    let mut groups: Vec<Value> = Vec::new();
    let mut current_key: Option<serde_json::Value> = None;
    let mut current_values = Vec::new();
    for (key, value) in keyed {
        if current_key.as_ref().is_some_and(|current| *current != key) {
            groups.push(Value::Array(Array::List(std::mem::take(
                &mut current_values,
            ))));
        }
        current_key = Some(key);
        current_values.push(value);
    }
    if current_key.is_some() {
        groups.push(Value::Array(Array::List(current_values)));
    }

    Ok(Value::Array(Array::List(groups)))
}

fn evaluate_unique(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("unique cannot be applied to this value".to_owned());
    };

    let mut values = array.values();
    values.sort_by(|left, right| compare_key_json(&left.to_json_value(), &right.to_json_value()));
    values.dedup_by(|left, right| left.to_json_value() == right.to_json_value());
    Ok(Value::Array(Array::List(values)))
}

fn evaluate_min_max_by(
    input: &Value,
    filter: &Expr,
    env: &Env,
    max: bool,
) -> Result<Value, String> {
    let keyed = keyed_array_values(input, filter, env)?;
    let selected = if max {
        keyed
            .into_iter()
            .max_by(|left, right| compare_key_json(&left.0, &right.0))
    } else {
        keyed
            .into_iter()
            .min_by(|left, right| compare_key_json(&left.0, &right.0))
    };
    Ok(selected.map(|(_, value)| value).unwrap_or(Value::Null))
}

fn keyed_array_values(
    input: &Value,
    filter: &Expr,
    env: &Env,
) -> Result<Vec<(serde_json::Value, Value)>, String> {
    let Value::Array(array) = input else {
        if matches!(input, Value::Object(_)) {
            return Err(
                "object and array cannot be sorted, as they are not both arrays".to_owned(),
            );
        }
        return Err(format!("Cannot iterate over {}", value_kind(input)));
    };

    array
        .values()
        .into_iter()
        .map(|value| {
            let key = sort_key(filter, &value, env)?;
            Ok((key, value))
        })
        .collect()
}

fn sort_key(filter: &Expr, input: &Value, env: &Env) -> Result<serde_json::Value, String> {
    let values = filter.eval(input, env)?;
    Ok(serde_json::Value::Array(
        values
            .into_iter()
            .map(|value| value.to_json_value())
            .collect(),
    ))
}

fn evaluate_has(input: &Value, key: &Value) -> Result<Value, String> {
    match (input, key) {
        (Value::Array(array), Value::Number(index)) => {
            let index = parse_usize(index)?;
            Ok(Value::Bool(index < array.len()))
        }
        (Value::Object(document), Value::String(key)) => {
            Ok(Value::Bool(document.get(key).is_some()))
        }
        (Value::Object(document), Value::Number(key)) => {
            Ok(Value::Bool(document.get(key).is_some()))
        }
        _ => Err("has() cannot check this value".to_owned()),
    }
}

fn evaluate_to_entries(input: &Value) -> Result<Value, String> {
    let entries = match input {
        Value::Array(array) => array
            .values()
            .into_iter()
            .enumerate()
            .map(|(index, value)| entry_value(Value::Number(index.to_string()), value))
            .collect(),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            map.into_iter()
                .map(|(key, value)| entry_value(Value::String(key), Value::from_json_value(value)))
                .collect()
        }
        _ => return Err("value has no keys".to_owned()),
    };
    Ok(Value::Array(Array::List(entries)))
}

fn entry_value(key: Value, value: Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("key".to_owned(), key.to_json_value());
    object.insert("value".to_owned(), value.to_json_value());
    Value::from_json_value(serde_json::Value::Object(object))
}

fn evaluate_from_entries(input: &Value) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("from_entries cannot be applied to this value".to_owned());
    };

    let mut object = serde_json::Map::new();
    for entry in array.values() {
        let Value::Object(document) = entry else {
            return Err(format!(
                "Cannot index {} with string \"key\"",
                value_kind(&entry)
            ));
        };
        let key = document
            .get("key")
            .or_else(|| document.get("Key"))
            .or_else(|| document.get("name"))
            .or_else(|| document.get("Name"))
            .cloned()
            .unwrap_or(Value::Null);
        let value = document
            .get("value")
            .or_else(|| document.get("Value"))
            .cloned()
            .unwrap_or(Value::Null);
        object.insert(entry_key_string(&key)?, value.to_json_value());
    }

    Ok(Value::from_json_value(serde_json::Value::Object(object)))
}

pub(super) fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "array",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Number(_) => "number",
        Value::Object(_) => "object",
        Value::String(_) => "string",
    }
}

fn entry_key_string(value: &Value) -> Result<String, String> {
    match value {
        Value::Number(value) | Value::String(value) => Ok(value.clone()),
        _ => Err(format!("Cannot use {} as object key", value_kind(value))),
    }
}

fn evaluate_split(input: &Value, separator: &str) -> Result<Value, String> {
    let Value::String(value) = input else {
        return Err("split cannot be applied to this value".to_owned());
    };

    let values = if separator.is_empty() {
        value
            .chars()
            .map(|character| Value::String(character.to_string()))
            .collect()
    } else {
        value
            .split(separator)
            .map(|part| Value::String(part.to_owned()))
            .collect()
    };
    Ok(Value::Array(Array::List(values)))
}

fn evaluate_join(input: &Value, separator: &str) -> Result<Value, String> {
    let Value::Array(array) = input else {
        return Err("join cannot be applied to this value".to_owned());
    };

    let parts = array
        .values()
        .into_iter()
        .map(|value| match value {
            Value::Bool(value) => Ok(value.to_string()),
            Value::Null => Ok(String::new()),
            Value::Number(value) | Value::String(value) => Ok(value),
            _ => Err("join cannot stringify this value".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::String(parts.join(separator)))
}

fn single_string_arg(
    filter: &Expr,
    input: &Value,
    env: &Env,
    builtin: &str,
) -> Result<String, String> {
    let values = filter.eval(input, env)?;
    match values.as_slice() {
        [Value::String(value)] => Ok(value.clone()),
        [_] => Err(format!("{builtin} argument must be a string")),
        _ => Err(format!("{builtin} argument must produce one value")),
    }
}

fn evaluate_keys(input: &Value) -> Result<Value, String> {
    match input {
        Value::Array(array) => Ok(Value::Array(Array::List(
            (0..array.len())
                .map(|index| Value::Number(index.to_string()))
                .collect(),
        ))),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            Ok(Value::Array(Array::List(
                keys.into_iter().map(Value::String).collect(),
            )))
        }
        _ => Err("keys cannot be applied to this value".to_owned()),
    }
}

fn evaluate_length(input: &Value) -> Result<Value, String> {
    let length = match input {
        Value::Array(array) => array.len() as f64,
        Value::Null => 0.0,
        Value::Number(value) => parse_number(value)?.abs(),
        Value::Object(document) => {
            let serde_json::Value::Object(map) = document.to_json_value() else {
                unreachable!("document serializes as object");
            };
            map.len() as f64
        }
        Value::String(value) => value.chars().count() as f64,
        Value::Bool(_) => return Err("boolean has no length".to_owned()),
    };
    number_value(length)
}

pub(super) fn evaluate_binary(
    operator: BinaryOp,
    left: &Value,
    right: &Value,
) -> Result<Value, String> {
    match operator {
        BinaryOp::Add => add_values(left, right),
        BinaryOp::And => Ok(Value::Bool(is_truthy(left) && is_truthy(right))),
        BinaryOp::Subtract => subtract_values(left, right),
        BinaryOp::Multiply => number_value(parse_number_value(left)? * parse_number_value(right)?),
        BinaryOp::Or => Ok(Value::Bool(is_truthy(left) || is_truthy(right))),
        BinaryOp::Divide => {
            let divisor = parse_number_value(right)?;
            if divisor == 0.0 {
                return Err("division by zero".to_owned());
            }
            number_value(parse_number_value(left)? / divisor)
        }
        BinaryOp::Equal => Ok(Value::Bool(left.to_json_value() == right.to_json_value())),
        BinaryOp::NotEqual => Ok(Value::Bool(left.to_json_value() != right.to_json_value())),
        BinaryOp::Less => Ok(Value::Bool(compare_values(left, right)?.is_lt())),
        BinaryOp::LessEqual => Ok(Value::Bool(!compare_values(left, right)?.is_gt())),
        BinaryOp::Modulo => {
            let divisor = parse_number_value(right)?;
            if divisor == 0.0 {
                return Err("division by zero".to_owned());
            }
            number_value(parse_number_value(left)? % divisor)
        }
        BinaryOp::Greater => Ok(Value::Bool(compare_values(left, right)?.is_gt())),
        BinaryOp::GreaterEqual => Ok(Value::Bool(!compare_values(left, right)?.is_lt())),
    }
}

fn add_values(left: &Value, right: &Value) -> Result<Value, String> {
    match (left, right) {
        (Value::Null, value) | (value, Value::Null) => Ok(value.clone()),
        (Value::Number(left), Value::Number(right)) => {
            number_value(parse_number(left)? + parse_number(right)?)
        }
        (Value::String(left), Value::String(right)) => Ok(Value::String(format!("{left}{right}"))),
        (Value::Array(left), Value::Array(right)) => {
            let mut values = left.values();
            values.extend(right.values());
            Ok(Value::Array(Array::List(values)))
        }
        (Value::Object(_), Value::Object(_)) => {
            let serde_json::Value::Object(mut left) = left.to_json_value() else {
                unreachable!("object serializes as object");
            };
            let serde_json::Value::Object(right) = right.to_json_value() else {
                unreachable!("object serializes as object");
            };
            left.extend(right);
            Ok(Value::from_json_value(serde_json::Value::Object(left)))
        }
        _ => Err("cannot add these values".to_owned()),
    }
}

fn subtract_values(left: &Value, right: &Value) -> Result<Value, String> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            number_value(parse_number(left)? - parse_number(right)?)
        }
        (Value::Array(left), Value::Array(right)) => {
            let remove = right
                .values()
                .into_iter()
                .map(|value| value.to_json_value())
                .collect::<Vec<_>>();
            Ok(Value::Array(Array::List(
                left.values()
                    .into_iter()
                    .filter(|value| !remove.contains(&value.to_json_value()))
                    .collect(),
            )))
        }
        _ => Err("cannot subtract these values".to_owned()),
    }
}

fn compare_values(left: &Value, right: &Value) -> Result<std::cmp::Ordering, String> {
    let left_json = left.to_json_value();
    let right_json = right.to_json_value();
    compare_json_values(&left_json, &right_json)
}

pub(super) fn is_truthy(value: &Value) -> bool {
    !matches!(value, Value::Bool(false) | Value::Null)
}

fn parse_number_value(value: &Value) -> Result<f64, String> {
    match value {
        Value::Number(value) => parse_number(value),
        _ => Err("expected number".to_owned()),
    }
}

pub(super) fn parse_number(value: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|_| format!("invalid number `{value}`"))
}

fn parse_usize(value: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("invalid array index `{value}`"))
}

fn number_value(value: f64) -> Result<Value, String> {
    if !value.is_finite() {
        return Err("number is not finite".to_owned());
    }
    if value.fract() == 0.0 {
        Ok(Value::Number(format!("{value:.0}")))
    } else {
        serde_json::Number::from_f64(value)
            .map(|number| Value::Number(number.to_string()))
            .ok_or_else(|| "number is not finite".to_owned())
    }
}
