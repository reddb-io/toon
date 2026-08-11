use reddb_io_toon::Value;

use super::super::ast::Expr;
use super::super::eval::Env;
use super::Builtin;

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("abs", 0, call_abs),
    Builtin::new("ceil", 0, call_ceil),
    Builtin::new("exp", 0, call_exp),
    Builtin::new("fabs", 0, call_fabs),
    Builtin::new("floor", 0, call_floor),
    Builtin::new("log", 0, call_log),
    Builtin::new("log2", 0, call_log2),
    Builtin::new("log10", 0, call_log10),
    Builtin::new("pow", 2, call_pow),
    Builtin::new("round", 0, call_round),
    Builtin::new("significand", 0, call_significand),
    Builtin::new("sqrt", 0, call_sqrt),
    Builtin::new("trunc", 0, call_trunc),
];

fn call_abs(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, |value| if value < 0.0 { -value } else { value })
}

fn call_ceil(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::ceil)
}

fn call_exp(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::exp)
}

fn call_fabs(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::abs)
}

fn call_floor(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::floor)
}

fn call_log(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::ln)
}

fn call_log2(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::log2)
}

fn call_log10(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::log10)
}

fn call_pow(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let bases = arguments[0].eval(input, env)?;
    let exponents = arguments[1].eval(input, env)?;
    let mut output = Vec::new();
    for exponent in &exponents {
        let exponent = required_number(exponent)?;
        for base in &bases {
            output.push(number_value(required_number(base)?.powf(exponent)));
        }
    }
    Ok(output)
}

fn call_round(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::round)
}

fn call_significand(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, significand)
}

fn call_sqrt(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::sqrt)
}

fn call_trunc(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    call_unary(input, f64::trunc)
}

fn call_unary(input: &Value, operation: fn(f64) -> f64) -> Result<Vec<Value>, String> {
    Ok(vec![number_value(operation(required_number(input)?))])
}

fn required_number(value: &Value) -> Result<f64, String> {
    let Value::Number(value) = value else {
        return Err("number required".to_owned());
    };
    value.parse().map_err(|_| "number required".to_owned())
}

fn number_value(value: f64) -> Value {
    let encoded = if value.is_nan() {
        "NaN".to_owned()
    } else if value == f64::INFINITY {
        "inf".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_owned()
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        serde_json::Number::from_f64(value)
            .expect("finite f64 is a JSON number")
            .to_string()
    };
    Value::Number(encoded)
}

fn significand(value: f64) -> f64 {
    const FRACTION_MASK: u64 = (1_u64 << 52) - 1;
    const EXPONENT_ONE: u64 = 1023_u64 << 52;

    let bits = value.to_bits();
    let sign = bits & (1_u64 << 63);
    let exponent = bits & (0x7ff_u64 << 52);
    let fraction = bits & FRACTION_MASK;
    if exponent == 0 {
        if fraction == 0 {
            return value;
        }
        let highest = 63 - fraction.leading_zeros() as u64;
        let normalized = fraction << (52 - highest);
        return f64::from_bits(sign | EXPONENT_ONE | (normalized & FRACTION_MASK));
    }
    if exponent == 0x7ff_u64 << 52 {
        return value;
    }
    f64::from_bits(sign | EXPONENT_ONE | fraction)
}
