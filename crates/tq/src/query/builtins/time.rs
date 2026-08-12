//! UTC-only time builtins.
//!
//! `gmtime`, `mktime`, `now`, `todate`, and `fromdate` never consult the
//! process timezone. `strftime` supports the locale-independent directives
//! implemented by [`format_time`]. `strptime` deliberately accepts only
//! `%Y`, `%m`, `%d`, `%H`, `%M`, `%S`, `%%`, and literal separators; a year,
//! month, and day are required. This keeps results portable without adding a
//! date/time dependency.

use std::time::{SystemTime, UNIX_EPOCH};

use reddb_io_toon::{Array, Value};

use super::super::ast::Expr;
use super::super::eval::Env;
use super::Builtin;

const SECONDS_PER_DAY: f64 = 86_400.0;
const MAX_ABSOLUTE_DAYS: i64 = 365_000_000;
const MAX_ABSOLUTE_YEAR: i64 = 1_000_000;
const ISO_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin::new("fromdate", 0, call_fromdate),
    Builtin::new("gmtime", 0, call_gmtime),
    Builtin::new("mktime", 0, call_mktime),
    Builtin::new("now", 0, call_now),
    Builtin::new("strftime", 1, call_strftime),
    Builtin::new("strptime", 1, call_strptime),
    Builtin::new("todate", 0, call_todate),
];

#[derive(Clone, Copy, Debug, PartialEq)]
struct DateTime {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: f64,
    weekday: i64,
    year_day: i64,
}

fn call_now(_: &[Expr], _: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    };
    Ok(vec![number_value(seconds)])
}

fn call_gmtime(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let seconds = numeric_timestamp(input, "gmtime() requires numeric inputs")?;
    Ok(vec![datetime_value(datetime_from_timestamp(seconds)?)])
}

fn call_mktime(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let Value::Array(array) = input else {
        return Err("mktime requires array inputs".to_owned());
    };
    let datetime = parsed_datetime(array, "mktime requires parsed datetime inputs")?;
    Ok(vec![
        number_value(timestamp_from_datetime(datetime)? as f64),
    ])
}

fn call_todate(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let seconds = numeric_timestamp(input, "strftime/1 requires parsed datetime inputs")?;
    let datetime = datetime_from_timestamp(seconds)?;
    Ok(vec![Value::String(format_time(&datetime, ISO_FORMAT)?)])
}

fn call_fromdate(_: &[Expr], input: &Value, _: &Env) -> Result<Vec<Value>, String> {
    let Value::String(value) = input else {
        return Err("strptime/1 requires string inputs and arguments".to_owned());
    };
    let datetime = parse_time(value, ISO_FORMAT)?;
    Ok(vec![
        number_value(timestamp_from_datetime(datetime)? as f64),
    ])
}

fn call_strftime(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let datetime = datetime_input(input)?;
    let mut output = Vec::new();
    for format in arguments[0].eval(input, env)? {
        let Value::String(format) = format else {
            return Err("strftime/1 requires a string format".to_owned());
        };
        output.push(Value::String(format_time(&datetime, &format)?));
    }
    Ok(output)
}

fn call_strptime(arguments: &[Expr], input: &Value, env: &Env) -> Result<Vec<Value>, String> {
    let Value::String(value) = input else {
        return Err("strptime/1 requires string inputs and arguments".to_owned());
    };
    let mut output = Vec::new();
    for format in arguments[0].eval(input, env)? {
        let Value::String(format) = format else {
            return Err("strptime/1 requires string inputs and arguments".to_owned());
        };
        output.push(datetime_value(parse_time(value, &format)?));
    }
    Ok(output)
}

fn numeric_timestamp(input: &Value, type_error: &str) -> Result<f64, String> {
    let Value::Number(value) = input else {
        return Err(type_error.to_owned());
    };
    let seconds = value.parse::<f64>().map_err(|_| type_error.to_owned())?;
    if !seconds.is_finite() {
        return Err("error converting number of seconds since epoch to datetime".to_owned());
    }
    Ok(seconds)
}

fn datetime_input(input: &Value) -> Result<DateTime, String> {
    match input {
        Value::Number(_) => datetime_from_timestamp(numeric_timestamp(
            input,
            "strftime/1 requires parsed datetime inputs",
        )?),
        Value::Array(array) => parsed_datetime(array, "strftime/1 requires parsed datetime inputs"),
        _ => Err("strftime/1 requires parsed datetime inputs".to_owned()),
    }
}

fn parsed_datetime(array: &Array, error: &str) -> Result<DateTime, String> {
    let values = array.values();
    if values.len() < 8 {
        return Err(error.to_owned());
    }
    let mut parts = [0_i64; 8];
    for (slot, value) in parts.iter_mut().zip(&values) {
        *slot = integer_component(value).ok_or_else(|| error.to_owned())?;
    }
    Ok(DateTime {
        year: parts[0],
        month: parts[1],
        day: parts[2],
        hour: parts[3],
        minute: parts[4],
        second: parts[5] as f64,
        weekday: parts[6],
        year_day: parts[7],
    })
}

fn integer_component(value: &Value) -> Option<i64> {
    let Value::Number(value) = value else {
        return None;
    };
    let number = value.parse::<f64>().ok()?;
    (number.is_finite() && number >= i64::MIN as f64 && number <= i64::MAX as f64)
        .then(|| number.trunc() as i64)
}

fn datetime_from_timestamp(seconds: f64) -> Result<DateTime, String> {
    let days_as_float = (seconds / SECONDS_PER_DAY).floor();
    if !(-MAX_ABSOLUTE_DAYS as f64..=MAX_ABSOLUTE_DAYS as f64).contains(&days_as_float) {
        return Err("error converting number of seconds since epoch to datetime".to_owned());
    }
    let days = days_as_float as i64;
    let within_day = seconds - days_as_float * SECONDS_PER_DAY;
    let hour = (within_day / 3_600.0).floor() as i64;
    let minute = ((within_day - hour as f64 * 3_600.0) / 60.0).floor() as i64;
    let second = within_day - hour as f64 * 3_600.0 - minute as f64 * 60.0;
    let (year, month, day) = civil_from_days(days);
    let year_day = days - days_from_civil(year, 1, 1)?;
    Ok(DateTime {
        year,
        month: month - 1,
        day,
        hour,
        minute,
        second,
        weekday: (days + 4).rem_euclid(7),
        year_day,
    })
}

fn timestamp_from_datetime(datetime: DateTime) -> Result<i128, String> {
    let total_months = i128::from(datetime.year)
        .checked_mul(12)
        .and_then(|value| value.checked_add(i128::from(datetime.month)))
        .ok_or_else(mktime_conversion_error)?;
    let year = total_months.div_euclid(12);
    if year < i128::from(-MAX_ABSOLUTE_YEAR) || year > i128::from(MAX_ABSOLUTE_YEAR) {
        return Err(mktime_conversion_error());
    }
    let month = total_months.rem_euclid(12) as i64 + 1;
    let days = i128::from(days_from_civil(year as i64, month, 1)?)
        .checked_add(i128::from(datetime.day) - 1)
        .ok_or_else(mktime_conversion_error)?;
    days.checked_mul(86_400)
        .and_then(|value| value.checked_add(i128::from(datetime.hour) * 3_600))
        .and_then(|value| value.checked_add(i128::from(datetime.minute) * 60))
        .and_then(|value| value.checked_add(datetime.second.trunc() as i128))
        .ok_or_else(mktime_conversion_error)
}

fn mktime_conversion_error() -> String {
    "error converting broken-down time to number of seconds since epoch".to_owned()
}

// Howard Hinnant's civil-calendar algorithms, shifted to the Unix epoch.
fn days_from_civil(year: i64, month: i64, day: i64) -> Result<i64, String> {
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era.checked_mul(146_097)
        .and_then(|value| value.checked_add(day_of_era))
        .and_then(|value| value.checked_sub(719_468))
        .ok_or_else(mktime_conversion_error)
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = shifted_month + if shifted_month < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn datetime_value(datetime: DateTime) -> Value {
    Value::Array(Array::List(vec![
        integer_value(datetime.year),
        integer_value(datetime.month),
        integer_value(datetime.day),
        integer_value(datetime.hour),
        integer_value(datetime.minute),
        number_value(datetime.second),
        integer_value(datetime.weekday),
        integer_value(datetime.year_day),
    ]))
}

fn integer_value(value: i64) -> Value {
    Value::Number(value.to_string())
}

fn number_value(value: f64) -> Value {
    let encoded = if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        serde_json::Number::from_f64(value)
            .expect("time values are finite")
            .to_string()
    };
    Value::Number(encoded)
}

fn format_time(datetime: &DateTime, format: &str) -> Result<String, String> {
    let mut output = String::new();
    let mut chars = format.chars();
    while let Some(character) = chars.next() {
        if character != '%' {
            output.push(character);
            continue;
        }
        let Some(directive) = chars.next() else {
            output.push('%');
            break;
        };
        match directive {
            '%' => output.push('%'),
            'Y' => output.push_str(&datetime.year.to_string()),
            'y' => output.push_str(&format!("{:02}", datetime.year.rem_euclid(100))),
            'm' => output.push_str(&format!("{:02}", datetime.month + 1)),
            'd' => output.push_str(&format!("{:02}", datetime.day)),
            'e' => output.push_str(&format!("{:2}", datetime.day)),
            'H' => output.push_str(&format!("{:02}", datetime.hour)),
            'M' => output.push_str(&format!("{:02}", datetime.minute)),
            'S' => output.push_str(&format!("{:02}", datetime.second.trunc() as i64)),
            'j' => output.push_str(&format!("{:03}", datetime.year_day + 1)),
            'w' => output.push_str(&datetime.weekday.to_string()),
            'u' => output.push_str(
                &(if datetime.weekday == 0 {
                    7
                } else {
                    datetime.weekday
                })
                .to_string(),
            ),
            'F' => output.push_str(&format!(
                "{}-{:02}-{:02}",
                datetime.year,
                datetime.month + 1,
                datetime.day
            )),
            'R' => output.push_str(&format!("{:02}:{:02}", datetime.hour, datetime.minute)),
            'T' => output.push_str(&format!(
                "{:02}:{:02}:{:02}",
                datetime.hour,
                datetime.minute,
                datetime.second.trunc() as i64
            )),
            'z' | 'Z' => {
                return Err(format!(
                    "strftime directive `%{directive}` is unavailable in tq's UTC-only subset"
                ));
            }
            other => {
                output.push('%');
                output.push(other);
            }
        }
    }
    Ok(output)
}

fn parse_time(input: &str, format: &str) -> Result<DateTime, String> {
    let parsed = parse_time_fields(input, format)
        .and_then(ParsedFields::into_datetime)
        .ok_or_else(|| format!("date {input:?} does not match format {format:?}"))?;
    Ok(parsed)
}

#[derive(Default)]
struct ParsedFields {
    year: Option<i64>,
    month: Option<i64>,
    day: Option<i64>,
    hour: i64,
    minute: i64,
    second: i64,
}

impl ParsedFields {
    fn into_datetime(self) -> Option<DateTime> {
        let year = self.year?;
        let month = self.month?;
        let day = self.day?;
        if !(-MAX_ABSOLUTE_YEAR..=MAX_ABSOLUTE_YEAR).contains(&year)
            || !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || !(0..=23).contains(&self.hour)
            || !(0..=59).contains(&self.minute)
            || !(0..=60).contains(&self.second)
        {
            return None;
        }
        let days = days_from_civil(year, month, day).ok()?;
        Some(DateTime {
            year,
            month: month - 1,
            day,
            hour: self.hour,
            minute: self.minute,
            second: self.second as f64,
            weekday: (days + 4).rem_euclid(7),
            year_day: days - days_from_civil(year, 1, 1).ok()?,
        })
    }
}

fn parse_time_fields(input: &str, format: &str) -> Option<ParsedFields> {
    let mut input = input;
    let mut fields = ParsedFields::default();
    let mut format_chars = format.chars();
    while let Some(character) = format_chars.next() {
        if character != '%' {
            input = input.strip_prefix(character)?;
            continue;
        }
        match format_chars.next()? {
            '%' => input = input.strip_prefix('%')?,
            'Y' => fields.year = Some(take_year(&mut input)?),
            'm' => fields.month = Some(take_two_digits(&mut input)?),
            'd' => fields.day = Some(take_two_digits(&mut input)?),
            'H' => fields.hour = take_two_digits(&mut input)?,
            'M' => fields.minute = take_two_digits(&mut input)?,
            'S' => fields.second = take_two_digits(&mut input)?,
            _ => return None,
        }
    }
    input.is_empty().then_some(fields)
}

fn take_two_digits(input: &mut &str) -> Option<i64> {
    let bytes = input.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_digit() || !bytes[1].is_ascii_digit() {
        return None;
    }
    let value = i64::from(bytes[0] - b'0') * 10 + i64::from(bytes[1] - b'0');
    *input = &input[2..];
    Some(value)
}

fn take_year(input: &mut &str) -> Option<i64> {
    let bytes = input.as_bytes();
    let sign_length = usize::from(bytes.first() == Some(&b'-'));
    let digit_count = bytes[sign_length..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return None;
    }
    let length = sign_length + digit_count;
    let year = input[..length].parse().ok()?;
    *input = &input[length..];
    Some(year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_calendar_round_trips_epoch_boundaries() {
        for days in [-719_528, -719_468, -1, 0, 1, 11_016, 2_932_896] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), Ok(days));
        }
    }

    #[test]
    fn timestamps_use_floor_division_before_the_epoch() {
        assert_eq!(
            datetime_from_timestamp(-0.25),
            Ok(DateTime {
                year: 1969,
                month: 11,
                day: 31,
                hour: 23,
                minute: 59,
                second: 59.75,
                weekday: 3,
                year_day: 364,
            })
        );
    }

    #[test]
    fn mktime_normalizes_month_day_and_clock_components() {
        let datetime = DateTime {
            year: 1970,
            month: 12,
            day: 0,
            hour: 24,
            minute: 61,
            second: 61.9,
            weekday: 0,
            year_day: 0,
        };
        assert_eq!(timestamp_from_datetime(datetime), Ok(31_539_721));
    }

    #[test]
    fn formatting_is_portable_and_rejects_timezone_directives() {
        let datetime = datetime_from_timestamp(951_827_696.75).expect("valid timestamp");
        assert_eq!(
            format_time(&datetime, "%Y %y %m %d %e %H %M %S %j %w %u %F %R %T %% %Q"),
            Ok("2000 00 02 29 29 12 34 56 060 2 2 2000-02-29 12:34 12:34:56 % %Q".to_owned())
        );
        assert!(format_time(&datetime, "%z").is_err());
        assert!(format_time(&datetime, "%Z").is_err());
    }

    #[test]
    fn parsing_rejects_missing_fields_ranges_and_unsupported_directives() {
        assert!(parse_time("2000", "%Y").is_err());
        assert!(parse_time("2000-13-01", "%Y-%m-%d").is_err());
        assert!(parse_time("2000-01-01 24:00:00", "%Y-%m-%d %H:%M:%S").is_err());
        assert!(parse_time("Tuesday", "%A").is_err());
        assert!(parse_time("2000-01-01x", "%Y-%m-%d").is_err());
    }

    #[test]
    fn parsing_accepts_literals_percent_and_jq_style_calendar_normalization() {
        let parsed = parse_time("2023%02%29", "%Y%%%m%%%d").expect("supported subset");
        assert_eq!(parsed.weekday, 3);
        assert_eq!(parsed.year_day, 59);
        assert_eq!(timestamp_from_datetime(parsed), Ok(1_677_628_800));
    }

    #[test]
    fn timestamp_bounds_and_components_fail_cleanly() {
        assert!(datetime_from_timestamp(f64::INFINITY).is_err());
        assert!(datetime_from_timestamp((MAX_ABSOLUTE_DAYS as f64 + 1.0) * 86_400.0).is_err());
        assert_eq!(integer_component(&Value::String("1".to_owned())), None);
        assert_eq!(integer_component(&Value::Number("NaN".to_owned())), None);
    }
}
