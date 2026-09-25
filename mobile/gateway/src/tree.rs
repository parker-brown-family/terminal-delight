//! The desk's left bar, read from the session file.
//!
//! The host writes `sessions/<key>.toml` and does not read most of it; neither
//! does this. It walks the handful of fields the left bar draws from —
//! projects, groups, tabs, the leaves under each tab and the sticky note on
//! each leaf — and passes colours through as the strings they are. A field it
//! does not know is not an error: the file is written by windows newer than any
//! gateway.

use serde_json::{json, Value};

use crate::paths;

pub fn read(session: &str) -> Value {
    let path = paths::session_file(session);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return json!({"available": false, "why": format!("{} is not readable", path.display())});
    };
    let Ok(doc) = toml::from_str::<toml::Value>(&text) else {
        // A file caught mid-write. The next poll will read it whole.
        return json!({"available": false, "why": "the session file did not parse"});
    };
    let arr = |k: &str| doc.get(k).and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let projects: Vec<Value> = arr("projects")
        .iter()
        .map(|p| {
            json!({
                "id": p.get("id").and_then(toml::Value::as_integer),
                "name": p.get("name").and_then(toml::Value::as_str),
                "color": p.get("color").and_then(toml::Value::as_str),
                "collapsed": p.get("collapsed").and_then(toml::Value::as_bool),
            })
        })
        .collect();
    let groups: Vec<Value> = arr("groups")
        .iter()
        .map(|g| {
            json!({
                "id": g.get("id").and_then(toml::Value::as_integer),
                "name": g.get("name").and_then(toml::Value::as_str),
                "color": g.get("color").and_then(toml::Value::as_str),
                "text_color": g.get("text_color").and_then(toml::Value::as_str),
                "project": g.get("project").and_then(toml::Value::as_integer),
            })
        })
        .collect();
    let active = doc.get("active").and_then(toml::Value::as_integer);
    let tabs: Vec<Value> = arr("tabs")
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut leaves = vec![];
            if let Some(node) = t.get("node") {
                walk(node, &mut leaves);
            }
            json!({
                "index": i,
                "name": t.get("name").and_then(toml::Value::as_str),
                "group": t.get("group").and_then(toml::Value::as_integer),
                "project": t.get("project").and_then(toml::Value::as_integer),
                "active": active == Some(i as i64),
                "leaves": leaves,
            })
        })
        .collect();
    json!({"available": true, "projects": projects, "groups": groups, "tabs": tabs})
}

/// A node is `{Leaf = {...}}` or `{Split = {a = node, b = node, ...}}`.
fn walk(node: &toml::Value, out: &mut Vec<Value>) {
    if let Some(leaf) = node.get("Leaf") {
        let note = leaf.get("note").map(|n| {
            json!({
                "text": n.get("text").and_then(toml::Value::as_str),
                "pinned": n.get("pinned").and_then(toml::Value::as_bool),
            })
        });
        out.push(json!({
            "pane": leaf.get("pane_id").and_then(toml::Value::as_integer),
            "name": leaf.get("name").and_then(toml::Value::as_str),
            "cwd": leaf.get("cwd").and_then(toml::Value::as_str),
            "note": note,
        }));
    } else if let Some(split) = node.get("Split") {
        for side in ["a", "b"] {
            if let Some(child) = split.get(side) {
                walk(child, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_yields_its_leaves_in_reading_order() {
        let doc: toml::Value = toml::from_str(
            r#"
            [Split]
            ratio = 0.5
            [Split.a.Leaf]
            pane_id = 3
            [Split.b.Split.a.Leaf]
            pane_id = 4
            name = "logs"
            [Split.b.Split.b.Leaf]
            pane_id = 5
            [Split.b.Split.b.Leaf.note]
            text = "watch this"
            "#,
        )
        .unwrap();
        let mut out = vec![];
        walk(&doc, &mut out);
        let ids: Vec<i64> = out.iter().map(|l| l["pane"].as_i64().unwrap()).collect();
        assert_eq!(ids, [3, 4, 5]);
        assert_eq!(out[1]["name"], "logs");
        assert_eq!(out[2]["note"]["text"], "watch this");
        assert!(out[0]["note"].is_null(), "no note is absent, not empty");
    }
}
