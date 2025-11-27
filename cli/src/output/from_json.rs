//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::borrow::Cow;

use ort_openrouter_core::common::data::PromptOpts;

use crate::config::{ApiKey, ConfigFile, Settings};

impl ConfigFile {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut p = Parser::new(json);
        p.skip_ws();
        p.expect(b'{')?;

        let mut settings: Option<Settings> = None;
        let mut keys: Vec<ApiKey> = vec![];
        let mut prompt_opts: Option<crate::PromptOpts> = None;

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            let key = p
                .parse_simple_str()
                .inspect_err(|err| eprintln!("ConfigFile parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key {
                "settings" => {
                    if settings.is_some() {
                        return Err("duplicate field: settings".into());
                    }
                    let settings_json = p.value_slice()?;
                    settings = Some(Settings::from_json(settings_json)?);
                }
                "keys" => {
                    if !keys.is_empty() {
                        return Err("duplicate field: keys".into());
                    }
                    if !p.try_consume(b'[') {
                        return Err("keys: Expected array".into());
                    }
                    loop {
                        let j = p.value_slice()?;
                        let api_key = ApiKey::from_json(j)?;
                        keys.push(api_key);
                        p.skip_ws();
                        if p.try_consume(b',') {
                            continue;
                        }
                        p.skip_ws();
                        if p.try_consume(b']') {
                            break;
                        }
                    }
                }
                "prompt_opts" => {
                    if prompt_opts.is_some() {
                        return Err("duplicate field: prompt_opts".into());
                    }
                    let opts_json = p.value_slice()?;
                    prompt_opts = Some(PromptOpts::from_json(opts_json)?);
                }
                _ => return Err("unknown field".into()),
            }
            p.skip_ws();
            if p.try_consume(b',') {
                continue;
            }
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }
        }

        Ok(ConfigFile {
            settings,
            keys,
            prompt_opts,
        })
    }
}

impl Settings {
    pub fn from_json(json: &str) -> Result<Self, &'static str> {
        let mut p = Parser::new(json);
        p.skip_ws();
        p.expect(b'{')?;

        let mut save_to_file = None;
        let mut dns = vec![];

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            let key = p
                .parse_simple_str()
                .inspect_err(|err| eprintln!("Settings parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key {
                "save_to_file" => {
                    if save_to_file.is_some() {
                        return Err("duplicate field: save_to_file");
                    }
                    if p.peek_is_null() {
                        p.parse_null()?;
                        save_to_file = None;
                    } else {
                        save_to_file = Some(p.parse_bool()?);
                    }
                }
                "dns" => {
                    if !dns.is_empty() {
                        return Err("duplicate field: dns");
                    }
                    if !p.try_consume(b'[') {
                        return Err("dns: Expected array");
                    }
                    loop {
                        let addr = p.parse_string()?;
                        dns.push(addr);
                        p.skip_ws();
                        if p.try_consume(b',') {
                            continue;
                        }
                        p.skip_ws();
                        if p.try_consume(b']') {
                            break;
                        }
                    }
                }
                _ => return Err("unknown field"),
            }

            p.skip_ws();
            if p.try_consume(b',') {
                continue;
            }
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            // If neither comma nor closing brace, it's malformed.
            if !p.eof() {
                return Err("expected ',' or '}'");
            } else {
                return Err("unexpected end of input");
            }
        }

        let default = Settings::default();
        Ok(Settings {
            save_to_file: save_to_file.unwrap_or(default.save_to_file),
            dns,
        })
    }
}

impl ApiKey {
    pub fn from_json(json: &str) -> Result<Self, &'static str> {
        let mut p = Parser::new(json);
        p.skip_ws();
        p.expect(b'{')?;

        let mut name = None;
        let mut value = None;

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            let key = p
                .parse_simple_str()
                .inspect_err(|err| eprintln!("ApiKey parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key {
                "name" => {
                    if name.is_some() {
                        return Err("duplicate field: name");
                    }
                    name = Some(
                        p.parse_string()
                            .inspect_err(|err| eprintln!("Parsing name: {err}"))?,
                    );
                }
                "value" => {
                    if value.is_some() {
                        return Err("duplicate field: value");
                    }
                    value = Some(
                        p.parse_string()
                            .inspect_err(|err| eprintln!("Parsing name: {err}"))?,
                    );
                }
                _ => return Err("unknown field"),
            }
            p.skip_ws();
            if p.try_consume(b',') {
                continue;
            } else {
                p.expect(b'}')?;
                break;
            }
        }

        Ok(ApiKey::new(
            name.expect("Missing ApiKey name"),
            value.expect("Missing ApiKey value"),
        ))
    }
}

// Minimal, fast JSON scanner tailored for our needs.
struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            s,
            b: s.as_bytes(),
            i: 0,
        }
    }

    fn eof(&self) -> bool {
        self.i >= self.b.len()
    }

    fn peek(&self) -> Option<u8> {
        if self.eof() {
            None
        } else {
            Some(self.b[self.i])
        }
    }

    fn try_consume(&mut self, ch: u8) -> bool {
        if self.peek() == Some(ch) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, ch: u8) -> Result<(), &'static str> {
        if self.try_consume(ch) {
            Ok(())
        } else {
            Err("expected character")
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\n' | b'\r' | b'\t' => self.i += 1,
                _ => break,
            }
        }
    }

    fn starts_with_bytes(&self, pat: &[u8]) -> bool {
        let end = self.i + pat.len();
        end <= self.b.len() && &self.b[self.i..end] == pat
    }

    fn parse_null(&mut self) -> Result<(), &'static str> {
        if self.starts_with_bytes(b"null") {
            self.i += 4;
            Ok(())
        } else {
            Err("expected null")
        }
    }

    fn peek_is_null(&self) -> bool {
        self.starts_with_bytes(b"null")
    }

    fn parse_bool(&mut self) -> Result<bool, &'static str> {
        self.skip_ws();
        if self.starts_with_bytes(b"true") {
            self.i += 4;
            Ok(true)
        } else if self.starts_with_bytes(b"false") {
            self.i += 5;
            Ok(false)
        } else {
            eprintln!(
                "Expected boolean, got '{}'",
                String::from_utf8_lossy(&self.b[self.i..])
            );
            Err("expected boolean")
        }
    }

    fn parse_simple_str(&mut self) -> Result<&'a str, &'static str> {
        self.skip_ws();
        if self.peek() != Some(b'"') {
            return Err("expected string");
        }
        self.i += 1;
        let start = self.i;
        let len = self.b.len();
        while self.i < len {
            let c = self.b[self.i];
            if c == b'\\' {
                // For maximum speed and simplicity, we reject escapes.
                return Err("string escapes are not supported");
            }
            if c == b'"' {
                let end = self.i;
                self.i += 1; // consume closing quote
                // Safety: start and end are at UTF-8 code point boundaries (quotes),
                // so slicing is valid even if contents contain non-ASCII.
                return Ok(&self.s[start..end]);
            }
            self.i += 1;
        }
        Err("unterminated string")
    }

    fn parse_string(&mut self) -> Result<String, &'static str> {
        self.skip_ws();
        if self.peek() != Some(b'"') {
            eprintln!(
                "expected string got: '{}'",
                String::from_utf8_lossy(&self.b[self.i..])
            );
            return Err("expected string");
        }
        let start = self.i + 1;
        let mut i = start;
        let len = self.b.len();

        // First pass: detect if we need to unescape
        let mut needs_unescape = false;
        while i < len {
            let c = self.b[i];
            if c == b'\\' {
                needs_unescape = true;
                break;
            }
            if c == b'"' {
                // no escapes
                let s = core::str::from_utf8(&self.b[start..i]).map_err(|_| "utf8 error")?;
                self.i = i + 1;
                return Ok(s.to_owned());
            }
            i += 1;
        }
        if !needs_unescape {
            return Err("unterminated string");
        }

        // Second pass: build with unescape
        let mut out = String::with_capacity((i - start) + 16);
        let mut seg_start = start;
        while i < len {
            let c = self.b[i];
            if c == b'\\' {
                // push preceding segment
                if i > seg_start {
                    out.push_str(
                        core::str::from_utf8(&self.b[seg_start..i]).map_err(|_| "utf8 error")?,
                    );
                }
                i += 1;
                if i >= len {
                    return Err("bad escape");
                }
                let e = self.b[i];
                match e {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000C}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let (cp, new_i) = self.parse_u_escape(i + 1)?;
                        i = new_i - 1; // -1 because loop will i += 1 at end
                        if let Some(ch) = core::char::from_u32(cp) {
                            out.push(ch);
                        } else {
                            return Err("invalid unicode");
                        }
                    }
                    _ => return Err("bad escape"),
                }
                i += 1;
                seg_start = i;
                continue;
            } else if c == b'"' {
                // end
                if i > seg_start {
                    out.push_str(
                        core::str::from_utf8(&self.b[seg_start..i]).map_err(|_| "utf8 error")?,
                    );
                }
                self.i = i + 1;
                return Ok(out);
            } else {
                i += 1;
            }
        }
        Err("unterminated string")
    }

    // Parses \uXXXX (with surrogate-pair handling). Input index points at first hex digit after 'u'.
    fn parse_u_escape(&self, i: usize) -> Result<(u32, usize), &'static str> {
        fn hex4(bytes: &[u8], i: usize) -> Result<(u16, usize), &'static str> {
            let end = i + 4;
            if end > bytes.len() {
                return Err("short \\u");
            }
            let mut v: u16 = 0;
            for b in bytes.iter().take(end).skip(i) {
                v = (v << 4) | hex_val(*b)?;
            }
            Ok((v, end))
        }
        fn hex_val(b: u8) -> Result<u16, &'static str> {
            match b {
                b'0'..=b'9' => Ok((b - b'0') as u16),
                b'a'..=b'f' => Ok((b - b'a' + 10) as u16),
                b'A'..=b'F' => Ok((b - b'A' + 10) as u16),
                _ => Err("bad hex"),
            }
        }

        let (first, i2) = hex4(self.b, i)?;
        let cp = first as u32;

        // Surrogate pair handling
        if (0xD800..=0xDBFF).contains(&first) {
            // Expect \uXXXX next
            if i2 + 2 > self.b.len() || self.b[i2] != b'\\' || self.b[i2 + 1] != b'u' {
                return Err("missing low surrogate");
            }
            let (second, i3) = hex4(self.b, i2 + 2)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err("invalid low surrogate");
            }
            let high = (first as u32) - 0xD800;
            let low = (second as u32) - 0xDC00;
            let code = 0x10000 + ((high << 10) | low);
            Ok((code, i3))
        } else if (0xDC00..=0xDFFF).contains(&first) {
            Err("unpaired low surrogate")
        } else {
            Ok((cp, i2))
        }
    }

    // Returns a slice of the next JSON value and advances past it.
    fn value_slice(&mut self) -> Result<&'a str, &'static str> {
        self.skip_ws();
        let start = self.i;
        let end = self.find_value_end()?;
        let out = &self.s[start..end];
        self.i = end;
        Ok(out)
    }

    fn find_value_end(&mut self) -> Result<usize, &'static str> {
        if self.eof() {
            return Err("unexpected end");
        }
        match self.b[self.i] {
            b'"' => self.scan_string_end(),
            b'{' => self.scan_brace_block(b'{', b'}'),
            b'[' => self.scan_brace_block(b'[', b']'),
            b't' => {
                if self.starts_with_bytes(b"true") {
                    Ok(self.i + 4)
                } else {
                    Err("bad literal")
                }
            }
            b'f' => {
                if self.starts_with_bytes(b"false") {
                    Ok(self.i + 5)
                } else {
                    Err("bad literal")
                }
            }
            b'n' => {
                if self.starts_with_bytes(b"null") {
                    Ok(self.i + 4)
                } else {
                    Err("bad literal")
                }
            }
            b'-' | b'0'..=b'9' => self.scan_number_end(),
            _ => Err("unexpected token"),
        }
    }

    fn scan_string_end(&self) -> Result<usize, &'static str> {
        let mut i = self.i + 1;
        let len = self.b.len();
        let mut escaped = false;
        while i < len {
            let c = self.b[i];
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if c == b'\\' {
                escaped = true;
                i += 1;
                continue;
            }
            if c == b'"' {
                return Ok(i + 1);
            }
            i += 1;
        }
        Err("unterminated string")
    }

    fn scan_brace_block(&self, open: u8, close: u8) -> Result<usize, &'static str> {
        let mut i = self.i;
        let len = self.b.len();
        let mut depth = 0usize;
        while i < len {
            let c = self.b[i];
            if c == b'"' {
                // Skip string
                let p = Parser {
                    s: self.s,
                    b: self.b,
                    i,
                };
                i = p.scan_string_end()?; // returns position after closing "
                continue;
            }
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            i += 1;
        }
        Err("unterminated structure")
    }

    fn scan_number_end(&self) -> Result<usize, &'static str> {
        let len = self.b.len();
        let mut i = self.i;

        if self.b[i] == b'-' {
            i += 1;
            if i >= len {
                return Err("bad number");
            }
        }

        // int part
        match self.b[i] {
            b'0' => {
                i += 1;
            }
            b'1'..=b'9' => {
                i += 1;
                while i < len {
                    match self.b[i] {
                        b'0'..=b'9' => i += 1,
                        _ => break,
                    }
                }
            }
            _ => return Err("bad number"),
        }

        // frac
        if i < len && self.b[i] == b'.' {
            i += 1;
            if i >= len || !self.b[i].is_ascii_digit() {
                return Err("bad number");
            }
            while i < len && self.b[i].is_ascii_digit() {
                i += 1;
            }
        }

        // exp
        if i < len && (self.b[i] == b'e' || self.b[i] == b'E') {
            i += 1;
            if i < len && (self.b[i] == b'+' || self.b[i] == b'-') {
                i += 1;
            }
            if i >= len || !self.b[i].is_ascii_digit() {
                return Err("bad number");
            }
            while i < len && self.b[i].is_ascii_digit() {
                i += 1;
            }
        }

        Ok(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key() {
        let s = r#"{"name":"openrouter","value":"sk-or-v1-a123b456c789d012a345b8032470394876576573242374098174093274abcdef"}"#;
        let got = ApiKey::from_json(s).unwrap();
        let expect = ApiKey::new(
            "openrouter".to_string(),
            "sk-or-v1-a123b456c789d012a345b8032470394876576573242374098174093274abcdef".to_string(),
        );
        assert_eq!(got, expect);
    }

    #[test]
    fn settings() {
        let s = r#"{
    "save_to_file": true,
    "dns": ["104.18.2.115", "104.18.3.115"]
}"#;
        let settings = Settings::from_json(s).unwrap();
        assert!(settings.save_to_file);
        assert_eq!(settings.dns, ["104.18.2.115", "104.18.3.115"]);
    }

    #[test]
    fn config_file() {
        let s = r#"
{
    "keys": [{"name": "openrouter", "value": "sk-or-v1-abcd1234"}],
    "settings": {
        "save_to_file": true,
        "dns": ["104.18.2.115", "104.18.3.115"]
    },
    "prompt_opts": {
        "model": "google/gemma-3n-e4b-it:free",
        "system": "Make your answer concise but complete. No yapping. Direct professional tone. No emoji.",
        "quiet": false,
        "show_reasoning": false,
        "reasoning": {
            "enabled": false
        }
    }
}
"#;
        let cfg = ConfigFile::from_json(s).unwrap();
        assert_eq!(cfg.keys.len(), 1);
        assert!(cfg.settings.is_some());
        assert!(cfg.prompt_opts.is_some());
    }
}
