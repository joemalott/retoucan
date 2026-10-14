//! Parser for the Lua table literals Lightroom stores develop settings in,
//! e.g. `s = { Exposure2012 = 0.35, ToneCurvePV2012 = { 0, 0, 255, 255 }, }`.
//! Tables with only positional entries become JSON arrays; anything else
//! becomes an object (positional entries keyed "1", "2", ...).

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Number, Value};

pub fn parse(text: &str) -> Result<Value> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    p.ws();
    // Optional `name =` prefix.
    let save = p.i;
    if p.ident().is_some() {
        p.ws();
        if p.peek() == Some(b'=') {
            p.i += 1;
        } else {
            p.i = save;
        }
    }
    p.ws();
    p.value()
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.i += 1;
            } else if c == b'-' && self.s.get(self.i + 1) == Some(&b'-') {
                while let Some(c) = self.peek() {
                    self.i += 1;
                    if c == b'\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn ident(&mut self) -> Option<String> {
        let start = self.i;
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => self.i += 1,
            _ => return None,
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') {
            self.i += 1;
        }
        Some(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }

    fn value(&mut self) -> Result<Value> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.table(),
            Some(b'"') | Some(b'\'') => Ok(Value::String(self.string()?)),
            Some(b'[') if matches!(self.s.get(self.i + 1), Some(b'[') | Some(b'=')) => {
                Ok(Value::String(self.long_string()?))
            }
            Some(c) if c == b'-' || c == b'.' || c.is_ascii_digit() => self.number(),
            Some(_) => {
                let id = self.ident().ok_or_else(|| anyhow!("unexpected byte at {}", self.i))?;
                Ok(match id.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    "nil" => Value::Null,
                    _ => Value::String(id),
                })
            }
            None => bail!("unexpected end of input"),
        }
    }

    fn number(&mut self) -> Result<Value> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        if self.s[self.i..].starts_with(b"0x") || self.s[self.i..].starts_with(b"0X") {
            self.i += 2;
            let hs = self.i;
            while matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                self.i += 1;
            }
            let v = i64::from_str_radix(std::str::from_utf8(&self.s[hs..self.i])?, 16)?;
            let neg = self.s[start] == b'-';
            return Ok(Value::Number((if neg { -v } else { v }).into()));
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            // Only allow a sign right after an exponent marker.
            let c = self.peek().unwrap();
            if (c == b'+' || c == b'-') && !matches!(self.s[self.i - 1], b'e' | b'E') {
                break;
            }
            self.i += 1;
        }
        let t = std::str::from_utf8(&self.s[start..self.i])?;
        if let Ok(v) = t.parse::<i64>() {
            return Ok(Value::Number(v.into()));
        }
        let f: f64 = t.parse().map_err(|_| anyhow!("bad number {t:?}"))?;
        Ok(Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null))
    }

    fn string(&mut self) -> Result<String> {
        let q = self.peek().unwrap();
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let c = self.peek().ok_or_else(|| anyhow!("unterminated string"))?;
            self.i += 1;
            if c == q {
                break;
            }
            if c == b'\\' {
                let e = self.peek().ok_or_else(|| anyhow!("bad escape"))?;
                self.i += 1;
                match e {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'0'..=b'9' => {
                        let mut v = (e - b'0') as u32;
                        for _ in 0..2 {
                            match self.peek() {
                                Some(d) if d.is_ascii_digit() => {
                                    v = v * 10 + (d - b'0') as u32;
                                    self.i += 1;
                                }
                                _ => break,
                            }
                        }
                        out.push(v as u8);
                    }
                    other => out.push(other),
                }
            } else {
                out.push(c);
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn long_string(&mut self) -> Result<String> {
        self.i += 1;
        let mut level = 0;
        while self.peek() == Some(b'=') {
            level += 1;
            self.i += 1;
        }
        if self.peek() != Some(b'[') {
            bail!("bad long string");
        }
        self.i += 1;
        let close: Vec<u8> = std::iter::once(b']')
            .chain(std::iter::repeat_n(b'=', level))
            .chain(std::iter::once(b']'))
            .collect();
        let start = self.i;
        let rel = self.s[start..]
            .windows(close.len())
            .position(|w| w == close.as_slice())
            .ok_or_else(|| anyhow!("unterminated long string"))?;
        self.i = start + rel + close.len();
        let mut body = &self.s[start..start + rel];
        if body.first() == Some(&b'\n') {
            body = &body[1..];
        }
        Ok(String::from_utf8_lossy(body).into_owned())
    }

    fn table(&mut self) -> Result<Value> {
        self.i += 1;
        let mut positional = Vec::new();
        let mut keyed = Map::new();
        loop {
            self.ws();
            match self.peek() {
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                Some(b',') | Some(b';') => {
                    self.i += 1;
                    continue;
                }
                None => bail!("unterminated table"),
                _ => {}
            }
            // `[key] = value`
            if self.peek() == Some(b'[') && !matches!(self.s.get(self.i + 1), Some(b'[') | Some(b'=')) {
                self.i += 1;
                let k = self.value()?;
                self.ws();
                if self.peek() != Some(b']') {
                    bail!("expected ] at {}", self.i);
                }
                self.i += 1;
                self.ws();
                if self.peek() != Some(b'=') {
                    bail!("expected = at {}", self.i);
                }
                self.i += 1;
                let v = self.value()?;
                let key = match k {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                keyed.insert(key, v);
                continue;
            }
            // `name = value` or a positional value.
            let save = self.i;
            if let Some(id) = self.ident() {
                self.ws();
                if self.peek() == Some(b'=') && self.s.get(self.i + 1) != Some(&b'=') {
                    self.i += 1;
                    let v = self.value()?;
                    keyed.insert(id, v);
                    continue;
                }
                self.i = save;
            }
            positional.push(self.value()?);
        }
        if keyed.is_empty() {
            return Ok(Value::Array(positional));
        }
        for (n, v) in positional.into_iter().enumerate() {
            keyed.insert((n + 1).to_string(), v);
        }
        Ok(Value::Object(keyed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_develop_settings() {
        let v = parse(
            "s = { Exposure2012 = 0.35,\nCameraProfile = \"Adobe Color\",\nConvertToGrayscale = false,\nToneCurve = { 0, 0, 255, 255 },\n[\"Odd Key\"] = -1.5e-2,\nNested = { { a = 1 }, },\n}",
        )
        .unwrap();
        assert_eq!(v["Exposure2012"], 0.35);
        assert_eq!(v["CameraProfile"], "Adobe Color");
        assert_eq!(v["ConvertToGrayscale"], false);
        assert_eq!(v["ToneCurve"][2], 255);
        assert_eq!(v["Odd Key"], -0.015);
        assert_eq!(v["Nested"][0]["a"], 1);
    }

    #[test]
    fn parses_escapes_and_empty_tables() {
        let v = parse("s = { A = \"x\\\"y\\n\", B = {  }, C = [[long\nstr]] }").unwrap();
        assert_eq!(v["A"], "x\"y\n");
        assert_eq!(v["B"], Value::Array(vec![]));
        assert_eq!(v["C"], "long\nstr");
    }
}
