//! JSON that keeps the document's key order, for editing another program's
//! settings file. The crate's `serde_json::Value` sorts keys (no
//! `preserve_order`), so a round trip through it would reshuffle a person's
//! whole `settings.json` to take out one hook. This keeps keys in the order
//! they were written and prints with the file's own indentation, so the only
//! bytes that change are the ones the edit removes.
use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum J {
    Null,
    Bool(bool),
    Num(serde_json::Number),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    pub(crate) fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn get_mut(&mut self, key: &str) -> Option<&mut J> {
        match self {
            J::Obj(kv) => kv.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn remove(&mut self, key: &str) -> Option<J> {
        match self {
            J::Obj(kv) => kv
                .iter()
                .position(|(k, _)| k == key)
                .map(|i| kv.remove(i).1),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            J::Str(s) => Some(s),
            _ => None,
        }
    }
}

struct JVisitor;

impl<'de> Visitor<'de> for JVisitor {
    type Value = J;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_unit<E>(self) -> Result<J, E> {
        Ok(J::Null)
    }
    fn visit_none<E>(self) -> Result<J, E> {
        Ok(J::Null)
    }
    fn visit_bool<E>(self, b: bool) -> Result<J, E> {
        Ok(J::Bool(b))
    }
    fn visit_i64<E>(self, n: i64) -> Result<J, E> {
        Ok(J::Num(n.into()))
    }
    fn visit_u64<E>(self, n: u64) -> Result<J, E> {
        Ok(J::Num(n.into()))
    }
    fn visit_f64<E: serde::de::Error>(self, n: f64) -> Result<J, E> {
        serde_json::Number::from_f64(n)
            .map(J::Num)
            .ok_or_else(|| E::custom("non-finite number"))
    }
    fn visit_str<E>(self, s: &str) -> Result<J, E> {
        Ok(J::Str(s.to_string()))
    }
    fn visit_string<E>(self, s: String) -> Result<J, E> {
        Ok(J::Str(s))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<J, A::Error> {
        let mut v = Vec::new();
        while let Some(x) = seq.next_element()? {
            v.push(x);
        }
        Ok(J::Arr(v))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<J, A::Error> {
        let mut v: Vec<(String, J)> = Vec::new();
        while let Some((k, x)) = map.next_entry::<String, J>()? {
            // A repeated key is read differently by different programs, so
            // what TD would remove and what the client runs could differ.
            if v.iter().any(|(seen, _)| *seen == k) {
                return Err(serde::de::Error::custom(format!(
                    "the key \"{k}\" appears twice"
                )));
            }
            v.push((k, x));
        }
        Ok(J::Obj(v))
    }
}

impl<'de> Deserialize<'de> for J {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<J, D::Error> {
        d.deserialize_any(JVisitor)
    }
}

impl Serialize for J {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            J::Null => s.serialize_unit(),
            J::Bool(b) => s.serialize_bool(*b),
            J::Num(n) => n.serialize(s),
            J::Str(x) => s.serialize_str(x),
            J::Arr(v) => {
                let mut q = s.serialize_seq(Some(v.len()))?;
                for x in v {
                    q.serialize_element(x)?;
                }
                q.end()
            }
            J::Obj(kv) => {
                let mut m = s.serialize_map(Some(kv.len()))?;
                for (k, x) in kv {
                    m.serialize_entry(k, x)?;
                }
                m.end()
            }
        }
    }
}

pub(crate) fn parse(text: &str) -> Result<J, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// Print `j` the way `original` was printed: its indentation (two spaces,
/// four, a tab, or a single line) and its trailing newline or lack of one.
pub(crate) fn print_like(j: &J, original: &str) -> String {
    let indent = original.lines().skip(1).find_map(|l| {
        let lead: String = l.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        (!lead.is_empty() && l[lead.len()..].starts_with(['"', '{', '[', '}', ']'])).then_some(lead)
    });
    let mut out = match indent {
        None => serde_json::to_string(j).unwrap_or_default(),
        Some(lead) => {
            let mut buf = Vec::new();
            let fmt = serde_json::ser::PrettyFormatter::with_indent(lead.as_bytes());
            let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
            let _ = j.serialize(&mut ser);
            String::from_utf8(buf).unwrap_or_default()
        }
    };
    if original.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_order_and_bytes() {
        let text = "{\n  \"zeta\": 1,\n  \"alpha\": {\n    \"b\": [\n      true,\n      null\n    ],\n    \"a\": \"x\"\n  },\n  \"mid\": 2.5\n}\n";
        let j = parse(text).unwrap();
        assert_eq!(print_like(&j, text), text);
    }

    #[test]
    fn four_space_tab_and_compact_files_keep_their_style() {
        for text in [
            "{\n    \"b\": 1,\n    \"a\": [\n        2\n    ]\n}",
            "{\n\t\"b\": 1\n}\n",
            "{\"b\":1,\"a\":[2]}",
        ] {
            assert_eq!(print_like(&parse(text).unwrap(), text), text, "{text:?}");
        }
    }

    #[test]
    fn a_repeated_key_is_refused() {
        assert!(parse(r#"{"hooks":{},"hooks":{"Stop":[]}}"#).is_err());
    }

    #[test]
    fn remove_and_lookup() {
        let mut j = parse(r#"{"a":1,"b":{"c":"d"}}"#).unwrap();
        assert_eq!(
            j.get("b").and_then(|b| b.get("c")).and_then(J::as_str),
            Some("d")
        );
        assert!(j.remove("a").is_some());
        assert_eq!(print_like(&j, "{}"), r#"{"b":{"c":"d"}}"#);
    }

    #[test]
    fn non_ascii_and_escapes_survive() {
        let text = "{\n  \"k\": \"caf\u{e9} \\\"q\\\" \\n\"\n}\n";
        assert_eq!(print_like(&parse(text).unwrap(), text), text);
    }
}
