//! Parser for the compact, LLM-oriented text format the OP.GG MCP server returns:
//!
//! ```text
//! class Data: core_items,boots
//! class CoreItems: ids,ids_names,play
//!
//! Data(CoreItems([3118],["Malignance"],37921),Boots([3020],["Sorcerer's Shoes"]))
//! ```
//!
//! Constructor names are reused across fields (e.g. starter items print as
//! `Boots(...)`), so a constructor's arguments are named by its `class` header's
//! field order, never by guessing from the constructor name. The result is a
//! plain `serde_json::Value` tree. Any deviation from the grammar is an error;
//! callers fall back to another build source.

use std::collections::HashMap;

use serde_json::{Map, Number, Value};

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Ctor(String, Vec<Node>),
    List(Vec<Node>),
    Num(Number),
    Str(String),
    Bool(bool),
    Null,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

const MAX_DEPTH: usize = 64;

impl<'a> Parser<'a> {
    fn err<T>(&self, msg: &str) -> Result<T, String> {
        Err(format!("{msg} at byte {}", self.i))
    }

    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ident(&mut self) -> String {
        let start = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_')
        {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.s[start..self.i]).into_owned()
    }

    /// Comma-separated values until `close`.
    fn seq(&mut self, close: u8) -> Result<Vec<Node>, String> {
        let mut out = Vec::new();
        self.ws();
        if self.peek() == Some(close) {
            self.i += 1;
            return Ok(out);
        }
        loop {
            out.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(c) if c == close => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => return self.err(&format!("expected ',' or '{}'", close as char)),
            }
        }
    }

    fn value(&mut self) -> Result<Node, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.err("nesting too deep");
        }
        self.ws();
        let node = match self.peek() {
            None => return self.err("unexpected end of input"),
            Some(b'[') => {
                self.i += 1;
                Node::List(self.seq(b']')?)
            }
            Some(b'"') => Node::Str(self.string()?),
            Some(c) if c == b'-' || c.is_ascii_digit() => Node::Num(self.number()?),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                let id = self.ident();
                self.ws();
                if self.peek() == Some(b'(') {
                    self.i += 1;
                    Node::Ctor(id, self.seq(b')')?)
                } else {
                    match id.as_str() {
                        "null" | "None" => Node::Null,
                        "true" | "True" => Node::Bool(true),
                        "false" | "False" => Node::Bool(false),
                        _ => return self.err(&format!("unknown bare word '{id}'")),
                    }
                }
            }
            Some(_) => return self.err("unexpected character"),
        };
        self.depth -= 1;
        Ok(node)
    }

    fn number(&mut self) -> Result<Number, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while self.i < self.s.len()
            && matches!(
                self.s[self.i],
                b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'
            )
        {
            self.i += 1;
        }
        let t = std::str::from_utf8(&self.s[start..self.i]).map_err(|e| e.to_string())?;
        if let Ok(n) = t.parse::<i64>() {
            return Ok(n.into());
        }
        match t.parse::<f64>().ok().and_then(Number::from_f64) {
            Some(n) => Ok(n),
            None => self.err(&format!("bad number '{t}'")),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // opening quote
        let mut out = String::new();
        let mut run_start = self.i;
        loop {
            match self.peek() {
                None => return self.err("unterminated string"),
                Some(b'"') => {
                    out.push_str(&String::from_utf8_lossy(&self.s[run_start..self.i]));
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    out.push_str(&String::from_utf8_lossy(&self.s[run_start..self.i]));
                    self.i += 1;
                    let c = self.peek().ok_or("unterminated escape")?;
                    self.i += 1;
                    match c {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'\'' => out.push('\''),
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'u' => {
                            let hex = self.s.get(self.i..self.i + 4).ok_or("short \\u escape")?;
                            let hex = std::str::from_utf8(hex).map_err(|e| e.to_string())?;
                            let cp = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
                            self.i += 4;
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        _ => return self.err("bad escape"),
                    }
                    run_start = self.i;
                }
                Some(_) => self.i += 1,
            }
        }
    }
}

/// Parse `class Name: a,b,c` header lines.
fn parse_headers(text: &str) -> (HashMap<String, Vec<String>>, usize) {
    let mut classes = HashMap::new();
    let mut body_start = 0;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("class ") {
            if let Some((name, fields)) = rest.split_once(':') {
                let fields = fields
                    .split(',')
                    .map(|f| f.trim().to_string())
                    .filter(|f| !f.is_empty())
                    .collect();
                classes.insert(name.trim().to_string(), fields);
            }
            body_start = offset + line.len();
        } else if !t.is_empty() {
            break;
        } else {
            body_start = offset + line.len();
        }
        offset += line.len();
    }
    (classes, body_start)
}

fn to_value(n: Node, classes: &HashMap<String, Vec<String>>) -> Result<Value, String> {
    Ok(match n {
        Node::Null => Value::Null,
        Node::Bool(b) => Value::Bool(b),
        Node::Num(n) => Value::Number(n),
        Node::Str(s) => Value::String(s),
        Node::List(items) => Value::Array(
            items
                .into_iter()
                .map(|i| to_value(i, classes))
                .collect::<Result<_, _>>()?,
        ),
        Node::Ctor(name, args) => {
            let fields = classes
                .get(&name)
                .ok_or_else(|| format!("no class header for '{name}'"))?;
            if fields.len() != args.len() {
                return Err(format!(
                    "'{name}' has {} fields but {} values",
                    fields.len(),
                    args.len()
                ));
            }
            let mut m = Map::new();
            for (f, a) in fields.iter().zip(args) {
                m.insert(f.clone(), to_value(a, classes)?);
            }
            Value::Object(m)
        }
    })
}

/// Parse a whole response text into a JSON tree.
pub fn parse(text: &str) -> Result<Value, String> {
    let (classes, body_start) = parse_headers(text);
    if classes.is_empty() {
        return Err("no class headers".into());
    }
    let mut p = Parser {
        s: &text.as_bytes()[body_start..],
        i: 0,
        depth: 0,
    };
    let node = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return p.err("trailing data");
    }
    to_value(node, &classes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example from the task, with the abbreviated skill list filled in.
    const EXAMPLE: &str = r#"class LolGetChampionAnalysis: data
class Data: core_items,boots,starter_items,summoner_spells,runes,skills
class CoreItems: ids,ids_names,play,win,pick_rate
class Boots: ids,ids_names
class Runes: id,primary_page_id,primary_rune_ids,secondary_page_id,secondary_rune_ids,stat_mod_ids,play,win,pick_rate
class Skills: order

LolGetChampionAnalysis(Data(CoreItems([3118,4645,3157],["Malignance","Shadowflame","Zhonya's Hourglass"],37921,20099,0.15),Boots([3020],["Sorcerer's Shoes"]),Boots([1056,2003,2003],["Doran's Ring","Health Potion","Health Potion"]),CoreItems([4,14],[4,14],291068,150268,0.71),Runes(8112,8100,[8112,8139,8140,8106],8200,[8210,8226],[5005,5008,5001],225190,114145,0.54),Skills(["W","Q","E","Q","Q","R","Q","W","Q","W","R","W","W","E","E"])))"#;

    #[test]
    fn parses_task_example() {
        let v = parse(EXAMPLE).unwrap();
        let d = &v["data"];
        assert_eq!(
            d["core_items"]["ids"],
            serde_json::json!([3118, 4645, 3157])
        );
        assert_eq!(d["core_items"]["ids_names"][2], "Zhonya's Hourglass");
        // Reused constructor names map by field position, not by name.
        assert_eq!(
            d["starter_items"]["ids"],
            serde_json::json!([1056, 2003, 2003])
        );
        assert_eq!(d["summoner_spells"]["ids"], serde_json::json!([4, 14]));
        assert_eq!(d["summoner_spells"]["play"], 291068);
        assert_eq!(d["runes"]["primary_page_id"], 8100);
        assert_eq!(
            d["runes"]["stat_mod_ids"],
            serde_json::json!([5005, 5008, 5001])
        );
        assert_eq!(d["runes"]["pick_rate"].as_f64(), Some(0.54));
        assert_eq!(d["skills"]["order"].as_array().unwrap().len(), 15);
    }

    #[test]
    fn parses_real_responses() {
        let v = parse(include_str!("../../fixtures/opgg-ahri-mid.txt")).unwrap();
        assert_eq!(v["data"]["fourth_items"].as_array().unwrap().len(), 3);
        assert_eq!(v["data"]["summary"]["positions"][0]["name"], "MID");
        let p = parse(include_str!("../../fixtures/opgg-profile.txt")).unwrap();
        let s = &p["data"]["summoner"];
        assert_eq!(s["league_stats"][1]["tier_info"]["tier"], Value::Null);
        // Header order (play,win,champion_name) differs from request order.
        assert_eq!(
            s["most_champions"]["champion_stats"][0]["champion_name"],
            "Yone"
        );
        assert_eq!(s["most_champions"]["champion_stats"][0]["play"], 63);
    }

    #[test]
    fn strings_and_escapes() {
        let v = parse("class A: s,t\nA(\"a \\\"q\\\" \\\\ \\u00e9\",\"Kai'Sa\")").unwrap();
        assert_eq!(v["s"], "a \"q\" \\ é");
        assert_eq!(v["t"], "Kai'Sa");
        let v = parse("class A: n,m,o\nA(-3,1.5e2,[])").unwrap();
        assert_eq!(v["n"], -3);
        assert_eq!(v["m"].as_f64(), Some(150.0));
        assert_eq!(v["o"], serde_json::json!([]));
    }

    #[test]
    fn malformed_inputs_error_without_panicking() {
        let bad = [
            "",
            "no headers here",
            "class A: x\n",
            "class A: x\nA(1",
            "class A: x\nA(1,2)", // arity mismatch
            "class A: x\nB(1)",   // unknown class
            "class A: x\nA(\"unterminated)",
            "class A: x\nA([1,2)",
            "class A: x\nA(1) trailing",
            "class A: x\nA(@)",
            "class A: x\nA(\"\\u12\")",
            "class A: x\nA(\"\\q\")",
            "class A: x\nA(1..2.3)",
            "class A: x\nA(bogus)",
        ];
        for b in bad {
            assert!(parse(b).is_err(), "should fail: {b:?}");
        }
        // The task's example verbatim (with a literal "..." abbreviation) is not valid.
        let abbreviated = EXAMPLE.replace(r#""E","E"])"#, r#"...])"#);
        assert!(parse(&abbreviated).is_err());
        // Deep nesting is rejected, not a stack overflow.
        let deep = format!("class A: x\n{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(parse(&deep).is_err());
    }
}
