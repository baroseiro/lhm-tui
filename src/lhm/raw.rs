//! Raw `/data.json` wire types and tolerant value parsing (SPEC §3.3–§3.4).
//!
//! Two LHM quirks are handled here by design — do not "simplify" them:
//! - **Q1**: `RawMin`/`RawValue`/`RawMax` are unit-suffixed *strings* up to
//!   0.9.6 and JSON *numbers* afterwards, so every value accepts both.
//! - **Q2**: the server may emit bare `NaN`/`Infinity` literals (invalid
//!   strict JSON) which [`sanitize`] rewrites to `null` before parsing.

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

/// A `data.json` node. Only some fields are present per node variant:
/// hardware nodes carry `HardwareId`, sensor nodes carry `SensorId`/`Type`,
/// type nodes (`Temperatures`, `Load`, …) carry neither.
#[derive(Debug, Clone, Deserialize)]
pub struct RawNode {
    #[serde(rename = "Text")]
    pub text: Option<String>,
    #[serde(rename = "Min", default)]
    pub min: RawNumber,
    #[serde(rename = "Value", default)]
    pub value: RawNumber,
    #[serde(rename = "Max", default)]
    pub max: RawNumber,
    #[serde(rename = "ImageURL")]
    pub image_url: Option<String>,
    #[serde(rename = "Children", default)]
    pub children: Vec<RawNode>,
    #[serde(rename = "HardwareId")]
    pub hardware_id: Option<String>,
    #[serde(rename = "SensorId")]
    pub sensor_id: Option<String>,
    #[serde(rename = "Type")]
    pub sensor_type: Option<String>,
    #[serde(rename = "RawMin", default)]
    pub raw_min: RawNumber,
    #[serde(rename = "RawValue", default)]
    pub raw_value: RawNumber,
    #[serde(rename = "RawMax", default)]
    pub raw_max: RawNumber,
    /// LHM version, present on the root node only (SPEC Q7).
    #[serde(rename = "Version")]
    pub version: Option<String>,
}

/// A value that may arrive as a JSON number, a unit-suffixed string
/// (`"46.9 °C"`), an empty string, or `null`. The original string is kept for
/// verbatim display.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawNumber {
    pub value: Option<f64>,
    pub text: Option<String>,
}

impl RawNumber {
    pub fn number(value: f64) -> Self {
        Self {
            value: Some(value),
            text: None,
        }
    }
}

impl<'de> Deserialize<'de> for RawNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RawNumberVisitor;

        impl<'de> Visitor<'de> for RawNumberVisitor {
            type Value = RawNumber;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a number, a unit-suffixed numeric string, or null")
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<RawNumber, E> {
                Ok(RawNumber {
                    value: finite(value),
                    text: None,
                })
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<RawNumber, E> {
                Ok(RawNumber {
                    value: Some(value as f64),
                    text: None,
                })
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<RawNumber, E> {
                Ok(RawNumber {
                    value: Some(value as f64),
                    text: None,
                })
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<RawNumber, E> {
                Ok(RawNumber {
                    value: parse_lenient_number(value),
                    text: Some(value.to_string()),
                })
            }

            fn visit_unit<E: de::Error>(self) -> Result<RawNumber, E> {
                Ok(RawNumber::default())
            }

            fn visit_none<E: de::Error>(self) -> Result<RawNumber, E> {
                Ok(RawNumber::default())
            }

            fn visit_some<D2>(self, deserializer: D2) -> Result<RawNumber, D2::Error>
            where
                D2: Deserializer<'de>,
            {
                deserializer.deserialize_any(RawNumberVisitor)
            }
        }

        deserializer.deserialize_any(RawNumberVisitor)
    }
}

fn finite(value: f64) -> Option<f64> {
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

/// Extract a number from a formatted LHM value (SPEC Q1):
/// leading float, optional sign, decimal comma tolerated, `NaN`/`Infinity`
/// text rejected. Returns `None` when there is nothing numeric.
pub fn parse_lenient_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.eq_ignore_ascii_case("nan")
        || text.eq_ignore_ascii_case("-nan")
        || text.eq_ignore_ascii_case("infinity")
        || text.eq_ignore_ascii_case("-infinity")
    {
        return None;
    }
    if let Ok(value) = text.parse::<f64>() {
        return finite(value);
    }
    // Decimal comma, but only when no dot is present ("46,9 °C" → 46.9).
    let normalized = if text.contains('.') {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.replace(',', "."))
    };
    if let Ok(value) = normalized.parse::<f64>() {
        return finite(value);
    }
    // Leading numeric token, tolerating unit suffixes ("1205 RPM").
    let mut end = 0;
    let mut seen_digit = false;
    for (index, c) in normalized.char_indices() {
        let keep = match c {
            '0'..='9' => {
                seen_digit = true;
                true
            }
            '+' | '-' if index == 0 => true,
            '.' | 'e' | 'E' => true,
            _ => false,
        };
        if !keep {
            break;
        }
        end = index + c.len_utf8();
    }
    if !seen_digit {
        return None;
    }
    normalized[..end].parse::<f64>().ok().and_then(finite)
}

/// Rewrite bare `NaN` / `Infinity` / `-Infinity` literals to `null` (SPEC Q2)
/// without touching string contents. Only whole value tokens are replaced.
pub fn sanitize(input: &str) -> Cow<'_, str> {
    if !input.contains("NaN") && !input.contains("Infinity") {
        return Cow::Borrowed(input);
    }

    let mut out = String::with_capacity(input.len() + 8);
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = input.char_indices();

    while let Some((index, c)) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }
        if matches!(c, 'N' | 'I' | '-') {
            let rest = &input[index..];
            let token_len = if rest.starts_with("NaN") {
                Some(3)
            } else if rest.starts_with("Infinity") {
                Some(8)
            } else if rest.starts_with("-Infinity") {
                Some(9)
            } else {
                None
            };
            if let Some(len) = token_len {
                let before_ok = out
                    .chars()
                    .rev()
                    .find(|ch| !ch.is_whitespace())
                    .is_none_or(|ch| matches!(ch, ':' | ',' | '['));
                let after_ok = rest[len..]
                    .chars()
                    .find(|ch| !ch.is_whitespace())
                    .is_none_or(|ch| matches!(ch, ',' | '}' | ']'));
                if before_ok && after_ok {
                    out.push_str("null");
                    for _ in 0..len - 1 {
                        chars.next();
                    }
                    continue;
                }
            }
        }
        out.push(c);
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_bare_nan_tokens() {
        assert_eq!(sanitize(r#"{"a":NaN}"#), r#"{"a":null}"#);
        assert_eq!(
            sanitize(r#"{"a":-Infinity,"b":Infinity}"#),
            r#"{"a":null,"b":null}"#
        );
        assert_eq!(sanitize("[NaN, 1.5]"), "[null, 1.5]");
        assert_eq!(sanitize("NaN"), "null");
        assert_eq!(sanitize(r#"{"a": NaN ,"b": 2}"#), r#"{"a": null ,"b": 2}"#);
    }

    #[test]
    fn sanitize_keeps_nan_inside_strings() {
        assert_eq!(sanitize(r#"{"t":"NaN °C"}"#), r#"{"t":"NaN °C"}"#);
        assert_eq!(
            sanitize(r#"{"t":"say Infinity now"}"#),
            r#"{"t":"say Infinity now"}"#
        );
        assert_eq!(sanitize(r#"{"t":"esc \" NaN"}"#), r#"{"t":"esc \" NaN"}"#);
    }

    #[test]
    fn sanitize_ignores_non_value_positions() {
        // Not a whole value token: leave untouched so parsing fails loudly.
        assert_eq!(sanitize(r#"{"a":NaNx}"#), r#"{"a":NaNx}"#);
        assert_eq!(sanitize(r#"{"NaN":1}"#), r#"{"NaN":1}"#);
    }

    #[test]
    fn sanitize_borrows_when_clean() {
        let clean = r#"{"a":1.5}"#;
        assert!(matches!(sanitize(clean), Cow::Borrowed(_)));
    }

    #[test]
    fn lenient_numbers_cover_units() {
        let cases = [
            ("46.9 °C", Some(46.9)),
            ("43.5 °C", Some(43.5)),
            ("-12.5 °C", Some(-12.5)),
            ("41.2 %", Some(41.2)),
            ("1205 RPM", Some(1205.0)),
            ("4275.0 MHz", Some(4275.0)),
            ("5.05 GHz", Some(5.05)),
            ("1.21 V", Some(1.21)),
            ("0.98 A", Some(0.98)),
            ("142.0 W", Some(142.0)),
            ("8619.9 MB", Some(8619.9)),
            ("20.1 GB", Some(20.1)),
            ("1234.5 KB/s", Some(1234.5)),
            ("1024 B/s", Some(1024.0)),
            ("35 L/h", Some(35.0)),
            ("12.3 µS/cm", Some(12.3)),
            ("28.4 dBA", Some(28.4)),
            ("15 mWh", Some(15.0)),
            ("46,9 °C", Some(46.9)),
            ("+5.0", Some(5.0)),
            ("42", Some(42.0)),
            ("", None),
            ("   ", None),
            ("—", None),
            ("N/A", None),
            ("NaN °C", None),
            ("Infinity", None),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_lenient_number(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn raw_number_deserializes_number_string_and_null() {
        let number: RawNumber = serde_json::from_str("46.9").expect("number");
        assert_eq!(number.value, Some(46.9));
        assert_eq!(number.text, None);

        let string: RawNumber = serde_json::from_str("\"46.9 °C\"").expect("string");
        assert_eq!(string.value, Some(46.9));
        assert_eq!(string.text.as_deref(), Some("46.9 °C"));

        let null: RawNumber = serde_json::from_str("null").expect("null");
        assert_eq!(null, RawNumber::default());

        let empty: RawNumber = serde_json::from_str("\"\"").expect("empty string");
        assert_eq!(empty.value, None);
        assert_eq!(empty.text.as_deref(), Some(""));
    }

    #[test]
    fn raw_node_parses_after_sanitize() {
        let text = r#"{"Version":"0.9.6","Text":"Sensor","Children":[
            {"Text":"Core","Min":"","Value":"46.9 °C","Max":"","SensorId":"/amdcpu/0/temperature/2",
             "Type":"Temperature","RawMin":NaN,"RawValue":NaN,"RawMax":null,"Children":[]}]}"#;
        let node: RawNode = serde_json::from_str(&sanitize(text)).expect("parses");
        let sensor = &node.children[0];
        assert_eq!(sensor.raw_value.value, None);
        assert_eq!(sensor.value.value, Some(46.9));
        assert_eq!(node.version.as_deref(), Some("0.9.6"));
    }
}
