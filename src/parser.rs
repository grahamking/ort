//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::str::FromStr;

use crate::{
    LastData, Message, Priority, PromptOpts, ReasoningConfig, ReasoningEffort, Role,
    config::{ApiKey, ConfigFile, Settings},
};

impl LastData {
    pub fn from_json(json: &str) -> Result<Self, &'static str> {
        let mut p = Parser::new(json);
        p.skip_ws();
        p.expect(b'{')?;

        let mut opts = None;
        let mut messages = vec![];

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            let key = p
                .parse_string()
                .inspect_err(|err| eprintln!("ApiKey parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key.as_str() {
                "opts" => {
                    if opts.is_some() {
                        return Err("duplicate field: opts");
                    }
                    let j = p.value_slice()?;
                    opts = Some(PromptOpts::from_json(j)?);
                }
                "messages" => {
                    if !messages.is_empty() {
                        return Err("duplicate field: messages");
                    }
                    if !p.try_consume(b'[') {
                        return Err("messages: Expected array");
                    }
                    loop {
                        let j = p.value_slice()?;
                        let msg = Message::from_json(j)?;
                        messages.push(msg);
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
        }

        Ok(LastData {
            opts: opts.expect("Missing prompt opts"),
            messages,
        })
    }
}

impl Message {
    pub fn from_json(json: &str) -> Result<Self, &'static str> {
        let mut p = Parser::new(json);
        p.skip_ws();
        p.expect(b'{')?;

        let mut role = None;
        let mut content = None;

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            let key = p
                .parse_string()
                .inspect_err(|err| eprintln!("ApiKey parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key.as_str() {
                "role" => {
                    if role.is_some() {
                        return Err("duplicate field: role");
                    }
                    let r = p.parse_string()?;
                    role = Some(Role::from_str(&r)?);
                }
                "content" => {
                    if content.is_some() {
                        return Err("duplicate field: content");
                    }
                    content = Some(p.parse_string()?);
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
        }

        Ok(Message::new(
            role.expect("Missing Role"),
            content.expect("Missing content"),
        ))
    }
}

impl ConfigFile {
    pub fn from_json(json: &str) -> Result<Self, &'static str> {
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
                .parse_string()
                .inspect_err(|err| eprintln!("ApiKey parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key.as_str() {
                "settings" => {
                    if settings.is_some() {
                        return Err("duplicate field: settings");
                    }
                    let settings_json = p.value_slice()?;
                    settings = Some(Settings::from_json(settings_json)?);
                }
                "keys" => {
                    if !keys.is_empty() {
                        return Err("duplicate field: keys");
                    }
                    if !p.try_consume(b'[') {
                        return Err("keys: Expected array");
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
                        return Err("duplicate field: prompt_opts");
                    }
                    let opts_json = p.value_slice()?;
                    prompt_opts = Some(PromptOpts::from_json(opts_json)?);
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
        let mut verify_certs = None;
        let mut dns = vec![];

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            let key = p
                .parse_string()
                .inspect_err(|err| eprintln!("ApiKey parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key.as_str() {
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
                "verify_certs" => {
                    if verify_certs.is_some() {
                        return Err("duplicate field: verify_certs");
                    }
                    if p.peek_is_null() {
                        p.parse_null()?;
                        verify_certs = None;
                    } else {
                        verify_certs = Some(p.parse_bool()?);
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
            verify_certs: verify_certs.unwrap_or(default.verify_certs),
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
                .parse_string()
                .inspect_err(|err| eprintln!("ApiKey parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key.as_str() {
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

impl ReasoningConfig {
    pub fn from_json(json: &str) -> Result<ReasoningConfig, &'static str> {
        let mut p = Parser::new(json);
        p.skip_ws();
        p.expect(b'{')?;

        let mut enabled: Option<bool> = None;
        let mut effort: Option<ReasoningEffort> = None;
        let mut tokens: Option<u32> = None;

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            // Key
            let key = p
                .parse_string()
                .inspect_err(|err| eprintln!("ReasoningConfig parsing key: {err}"))?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            // Value by key
            match key.as_str() {
                "enabled" => {
                    if enabled.is_some() {
                        return Err("duplicate field: enabled");
                    }
                    if p.peek_is_null() {
                        p.parse_null()?;
                        enabled = None;
                    } else {
                        enabled = Some(p.parse_bool()?);
                    }
                }
                "effort" => {
                    if effort.is_some() {
                        return Err("duplicate field: effort");
                    }
                    if p.peek_is_null() {
                        p.parse_null()?;
                        effort = None;
                    } else {
                        let v = p
                            .parse_string()
                            .inspect_err(|err| eprintln!("Parsing effort: {err}"))?;
                        let e = if v.eq_ignore_ascii_case("low") {
                            ReasoningEffort::Low
                        } else if v.eq_ignore_ascii_case("medium") {
                            ReasoningEffort::Medium
                        } else if v.eq_ignore_ascii_case("high") {
                            ReasoningEffort::High
                        } else {
                            return Err("invalid effort");
                        };
                        effort = Some(e);
                    }
                }
                "tokens" => {
                    if tokens.is_some() {
                        return Err("duplicate field: tokens");
                    }
                    if p.peek_is_null() {
                        p.parse_null()?;
                        tokens = None;
                    } else {
                        tokens = Some(p.parse_u32()?);
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

        p.skip_ws();
        if !p.eof() {
            return Err("trailing characters after JSON object");
        }

        let enabled = enabled.ok_or("missing required field: enabled")?;

        Ok(ReasoningConfig {
            enabled,
            effort,
            tokens,
        })
    }
}

impl PromptOpts {
    pub fn from_json(input: &str) -> Result<Self, &'static str> {
        let mut p = Parser::new(input);

        p.skip_ws();
        p.expect(b'{')?;

        let mut prompt: Option<String> = None;
        let mut model: Option<String> = None;
        let mut provider: Option<String> = None;
        let mut system: Option<String> = None;
        let mut priority: Option<Priority> = None;
        let mut reasoning: Option<ReasoningConfig> = None;
        let mut show_reasoning: Option<bool> = None;
        let mut quiet: Option<bool> = None;
        let mut merge_config = true;

        p.skip_ws();
        if p.try_consume(b'}') {
            return Ok(PromptOpts {
                prompt,
                model,
                provider,
                system,
                priority,
                reasoning,
                show_reasoning,
                quiet,
                merge_config,
            });
        }

        loop {
            p.skip_ws();
            let key = p.parse_string()?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key.as_str() {
                "prompt" => {
                    prompt = p.parse_opt_string()?;
                }
                "model" => {
                    model = p.parse_opt_string()?;
                }
                "provider" => {
                    provider = p.parse_opt_string()?;
                }
                "system" => {
                    system = p.parse_opt_string()?;
                }
                "priority" => {
                    if p.peek_is_null() {
                        p.parse_null()?;
                        priority = None;
                    } else {
                        let s = p.parse_string()?;
                        priority = Some(Priority::from_str(&s).map_err(|_| "invalid priority")?);
                    }
                }
                "reasoning" => {
                    if p.peek_is_null() {
                        p.parse_null()?;
                        reasoning = None;
                    } else {
                        // Grab the exact object slice and delegate to ReasoningConfig::from_json
                        let slice = p.value_slice()?; // must be an object
                        let cfg = ReasoningConfig::from_json(slice)
                            .inspect_err(|e| eprintln!("parser::PromptOpts::from_json {e}"))
                            .map_err(|_| "invalid reasoning")?;
                        reasoning = Some(cfg);
                    }
                }
                "show_reasoning" => {
                    if p.peek_is_null() {
                        p.parse_null()?;
                        show_reasoning = None;
                    } else {
                        show_reasoning = Some(p.parse_bool()?);
                    }
                }
                "quiet" => {
                    if p.peek_is_null() {
                        p.parse_null()?;
                        quiet = None;
                    } else {
                        quiet = Some(p.parse_bool()?);
                    }
                }
                "merge_config" => {
                    if p.peek_is_null() {
                        p.parse_null()?;
                        merge_config = true;
                    } else {
                        merge_config = p.parse_bool()?;
                    }
                }
                _ => {
                    // Unknown field: skip its value
                    p.skip_value()?;
                }
            }

            p.skip_ws();
            if p.try_consume(b',') {
                continue;
            } else {
                p.expect(b'}')?;
                break;
            }
        }

        Ok(PromptOpts {
            prompt,
            model,
            provider,
            system,
            priority,
            reasoning,
            show_reasoning,
            quiet,
            merge_config,
        })
    }
}

// Minimal, fast JSON scanner tailored for our needs.
struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    #[inline]
    fn new(s: &'a str) -> Self {
        Self {
            s,
            b: s.as_bytes(),
            i: 0,
        }
    }

    #[inline]
    fn eof(&self) -> bool {
        self.i >= self.b.len()
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        if self.eof() {
            None
        } else {
            Some(self.b[self.i])
        }
    }

    #[inline]
    fn try_consume(&mut self, ch: u8) -> bool {
        if self.peek() == Some(ch) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    #[inline]
    fn expect(&mut self, ch: u8) -> Result<(), &'static str> {
        if self.try_consume(ch) {
            Ok(())
        } else {
            Err("expected character")
        }
    }

    #[inline]
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\n' | b'\r' | b'\t' => self.i += 1,
                _ => break,
            }
        }
    }

    #[inline]
    fn starts_with_bytes(&self, pat: &[u8]) -> bool {
        let end = self.i + pat.len();
        end <= self.b.len() && &self.b[self.i..end] == pat
    }

    #[inline]
    fn parse_null(&mut self) -> Result<(), &'static str> {
        if self.starts_with_bytes(b"null") {
            self.i += 4;
            Ok(())
        } else {
            Err("expected null")
        }
    }

    #[inline]
    fn peek_is_null(&self) -> bool {
        self.starts_with_bytes(b"null")
    }

    #[inline]
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

    #[inline(always)]
    fn parse_u32(&mut self) -> Result<u32, &'static str> {
        self.skip_ws();
        if self.eof() {
            return Err("expected number");
        }
        if self.peek() == Some(b'-') {
            return Err("negative not allowed");
        }
        let mut val: u32 = 0;
        let mut read_any = false;
        let len = self.b.len();
        while self.i < len {
            let c = self.b[self.i];
            if c.is_ascii_digit() {
                read_any = true;
                let digit = (c - b'0') as u32;
                // Overflow-safe accumulation
                if val > (u32::MAX - digit) / 10 {
                    return Err("u32 overflow");
                }
                val = val * 10 + digit;
                self.i += 1;
            } else {
                break;
            }
        }
        if !read_any {
            return Err("expected integer");
        }
        Ok(val)
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
        #[inline]
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

    fn parse_opt_string(&mut self) -> Result<Option<String>, &'static str> {
        if self.peek_is_null() {
            self.parse_null()?;
            Ok(None)
        } else {
            let s = self.parse_string()?;
            Ok(Some(s))
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

    // Skips the next JSON value (string/number/boolean/null/object/array).
    fn skip_value(&mut self) -> Result<(), &'static str> {
        let _ = self.value_slice()?;
        Ok(())
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
    use crate::LastData;

    use super::*;

    #[test]
    fn rp1() {
        let cfg = ReasoningConfig::from_json(r#"{"enabled": false}"#).unwrap();
        assert!(!cfg.enabled);
        assert!(cfg.effort.is_none());
        assert!(cfg.tokens.is_none());
    }

    #[test]
    fn rp2() {
        let cfg = ReasoningConfig::from_json(r#"{"enabled": true, "effort": "medium"}"#).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.effort, Some(ReasoningEffort::Medium));
        assert!(cfg.tokens.is_none());
    }

    #[test]
    fn rp3() {
        let cfg = ReasoningConfig::from_json(r#"{"enabled": true, "tokens": 2048}"#).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.tokens, Some(2048));
        assert!(cfg.effort.is_none());
    }

    #[test]
    fn rp4() {
        let cfg = ReasoningConfig::from_json(r#"{"enabled":true,"effort":"high","tokens":null}"#)
            .unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.effort, Some(ReasoningEffort::High));
        assert!(cfg.tokens.is_none());
    }

    #[test]
    fn cpo1() {
        let s = r#"
 {
     "prompt": "\n\nExample JSON 1: {\"enabled\": false}\n",
     "model": "google/gemma-3n-e4b-it:free",
     "system": "Make your answer concise but complete. No yapping. Direct professional tone. No emoji.",
     "show_reasoning": false,
     "reasoning": { "enabled": false },
     "merge_config": true
 }
 "#;
        let opts = PromptOpts::from_json(s).unwrap();
        assert!(!opts.show_reasoning.unwrap());
        assert_eq!(opts.model.as_deref(), Some("google/gemma-3n-e4b-it:free"));
        assert!(!opts.reasoning.unwrap().enabled);
        assert!(opts.merge_config);
    }

    #[test]
    fn cpo2() {
        let s = r#"
    {"model":"openai/gpt-5","provider":"openai","system":"Make your answer concise but complete. No yapping. Direct professional tone. No emoji.","priority":null,"reasoning":{"enabled":true,"effort":"high","tokens":null},"show_reasoning":false,"quiet":true}
    "#;
        let opts = PromptOpts::from_json(s).unwrap();
        assert!(!opts.show_reasoning.unwrap());
        assert_eq!(opts.model.as_deref(), Some("openai/gpt-5"));
        assert!(opts.reasoning.as_ref().unwrap().enabled);
        assert_eq!(
            opts.reasoning.as_ref().unwrap().effort,
            Some(ReasoningEffort::High)
        );
    }

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

    #[test]
    fn last_data() {
        let s = r#"
{"opts":{"model":"google/gemma-3n-e4b-it:free","provider":"google-ai-studio","system":"Make your answer concise but complete. No yapping. Direct professional tone. No emoji.","priority":null,"reasoning":{"enabled":false,"effort":null,"tokens":null},"show_reasoning":false},"messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there! 😊How can I help you today? I'm ready for anything – questions, stories, ideas, or just a friendly chat!Let me know what's on your mind. ✨"}]}
"#;
        let l = LastData::from_json(s).unwrap();
        assert_eq!(l.opts.provider.as_deref(), Some("google-ai-studio"));
        assert_eq!(l.messages.len(), 2);
    }
}
