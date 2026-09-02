//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::common::utils;
use crate::{Context as _, ErrorKind, OrtResult, ort_err};

pub struct JsonField {
    pub name: &'static str,
    pub value: JsonValue,
}

impl JsonField {
    pub fn new_simple_string(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::SimpleString(None),
        }
    }

    pub fn new_string(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::String(None),
        }
    }

    pub fn new_int(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::Int(None),
        }
    }

    pub fn new_float(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::Float(None),
        }
    }

    pub fn new_bool(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::Bool(None),
        }
    }

    pub fn new_raw(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::Raw(None),
        }
    }

    pub fn new_vec_raw(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::VecRaw(None),
        }
    }

    fn parse(&mut self, p: &mut Parser) -> OrtResult<()> {
        match &mut self.value {
            JsonValue::SimpleString(inner) => {
                let s = p.parse_simple_str().map_err(|err| {
                    ort_err(
                        ErrorKind::FormatError,
                        ("Parsing field: ".to_string() + err.context.as_ref()).into(),
                    )
                })?;
                inner.replace(s.to_string());
            }
            JsonValue::String(inner) => {
                let s = p.parse_string().map_err(|err| {
                    ort_err(
                        ErrorKind::FormatError,
                        ("Parsing field: ".to_string() + err.context.as_ref()).into(),
                    )
                })?;
                inner.replace(s);
            }
            JsonValue::Int(inner) => {
                inner.replace(p.parse_u32()?);
            }
            JsonValue::Float(inner) => {
                inner.replace(p.parse_f32()?);
            }
            JsonValue::Bool(inner) => {
                inner.replace(p.parse_bool()?);
            }
            JsonValue::Raw(inner) => {
                let v = p.value_slice().context("JsonValue::Raw")?;
                // figure out lifetimes and keep as &str
                inner.replace(v.to_string());
            }
            JsonValue::VecRaw(inner) => {
                if !p.try_consume(b'[') {
                    return Err(ort_err(ErrorKind::FormatError, "Expected array".into()));
                }
                p.skip_ws();
                let mut v = vec![];
                // If the array isn't empty..
                if !p.try_consume(b']') {
                    loop {
                        let j = p.value_slice().context("JsonValue::VecRaw array")?;
                        v.push(j.to_string());
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
                inner.replace(v);
            }
        }
        Ok(())
    }

    pub fn get_string(&mut self) -> Option<String> {
        match &mut self.value {
            JsonValue::String(s) | JsonValue::SimpleString(s) => s.take(),
            _ => None,
        }
    }

    pub fn get_int(&mut self) -> Option<u32> {
        match &mut self.value {
            JsonValue::Int(i) => i.take(),
            _ => None,
        }
    }

    pub fn get_float(&mut self) -> Option<f32> {
        match &mut self.value {
            JsonValue::Float(f) => f.take(),
            _ => None,
        }
    }

    pub fn get_bool(&mut self) -> Option<bool> {
        match &mut self.value {
            JsonValue::Bool(b) => b.take(),
            _ => None,
        }
    }

    pub fn get_raw(&mut self) -> Option<String> {
        match &mut self.value {
            JsonValue::Raw(s) => s.take(),
            _ => None,
        }
    }

    pub fn get_vec_raw(&mut self) -> Option<Vec<String>> {
        match &mut self.value {
            JsonValue::VecRaw(v) => core::mem::take(v),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(
            self.value,
            JsonValue::String(None)
                | JsonValue::SimpleString(None)
                | JsonValue::Int(None)
                | JsonValue::Float(None)
                | JsonValue::Bool(None)
                | JsonValue::Raw(None)
                | JsonValue::VecRaw(None)
        )
    }
}

pub enum JsonValue {
    /// A string with no escapes. Faster to parse.
    SimpleString(Option<String>),

    /// Any string
    String(Option<String>),

    Int(Option<u32>),

    Float(Option<f32>),

    Bool(Option<bool>),

    /// Something the caller will parse themselves.
    /// Don't use for string because the delimiter is included, which
    /// for strings is '"'.
    Raw(Option<String>),

    /// Includes the delimiter ('[', '{', '"', etc).
    VecRaw(Option<Vec<String>>),
}

impl JsonValue {}

pub fn autoparser(json: &str, fields: &mut [JsonField]) -> OrtResult<()> {
    let mut p = Parser::new(json);
    p.skip_ws();
    p.expect(b'{')?;

    loop {
        p.skip_ws();
        if p.try_consume(b'}') {
            break;
        }

        let key = p.parse_simple_str().map_err(|err| {
            ort_err(
                ErrorKind::FormatError,
                ("parsing key: ".to_string() + err.context.as_ref()).into(),
            )
        })?;
        p.skip_ws();
        p.expect(b':')?;
        p.skip_ws();

        let mut is_parsed = false;
        for field in fields.iter_mut() {
            if field.name == key {
                if !field.is_empty() {
                    return Err(ort_err(
                        ErrorKind::FormatError,
                        ("duplicate field: ".to_string() + field.name).into(),
                    ));
                }
                if p.peek_is_null() {
                    p.skip_null()?;
                } else {
                    field.parse(&mut p)?;
                }
                is_parsed = true;
                break;
            }
        }
        if !is_parsed && let Err(err) = p.skip_value() {
            let msg = err.as_string() + ", not parsed w/ key: " + key;
            return Err(ort_err(ErrorKind::FormatError, msg.into()));
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

    p.skip_ws();
    if !p.eof() {
        return Err(ort_err(
            ErrorKind::FormatError,
            "trailing characters after JSON object".into(),
        ));
    }

    Ok(())
}

// --------------------------------------------

// Minimal, fast JSON scanner tailored for our needs.
pub struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            s,
            b: s.as_bytes(),
            i: 0,
        }
    }

    fn eof(&self) -> bool {
        self.i >= self.b.len()
    }

    pub fn peek(&self) -> Option<u8> {
        if self.eof() {
            None
        } else {
            Some(self.b[self.i])
        }
    }

    pub fn try_consume(&mut self, ch: u8) -> bool {
        if self.peek() == Some(ch) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    pub fn expect(&mut self, ch: u8) -> OrtResult<()> {
        if self.try_consume(ch) {
            Ok(())
        } else {
            Err(ort_err(ErrorKind::FormatError, "expected character".into()))
        }
    }

    pub fn skip_ws(&mut self) {
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

    pub fn skip_null(&mut self) -> OrtResult<()> {
        if self.starts_with_bytes(b"null") {
            self.i += 4;
            Ok(())
        } else {
            Err(ort_err(ErrorKind::FormatError, "expected null".into()))
        }
    }

    pub fn peek_is_null(&self) -> bool {
        self.starts_with_bytes(b"null")
    }

    fn parse_bool(&mut self) -> OrtResult<bool> {
        self.skip_ws();
        if self.starts_with_bytes(b"true") {
            self.i += 4;
            Ok(true)
        } else if self.starts_with_bytes(b"false") {
            self.i += 5;
            Ok(false)
        } else {
            Err(ort_err(
                ErrorKind::FormatError,
                ("Expected boolean, got: ".to_string()
                    + &String::from_utf8_lossy(&self.b[self.i..]))
                    .into(),
            ))
        }
    }

    fn parse_u32(&mut self) -> OrtResult<u32> {
        self.skip_ws();
        if self.eof() {
            return Err(ort_err(ErrorKind::FormatError, "expected number".into()));
        }
        if self.peek() == Some(b'-') {
            return Err(ort_err(
                ErrorKind::FormatError,
                "negative not allowed".into(),
            ));
        }
        let out = crate::common::utils::parse_u32(&self.b[self.i..])
            .map_err(|err| ort_err(ErrorKind::FormatError, err.into()))?;
        self.i += if out == 0 { 1 } else { out.ilog10() + 1 } as usize;

        // Skip any float part in case it wasn't really an int
        // This can happy dring tool calls
        if self.b[self.i] == b'.' {
            self.i += 1;
            while self.b[self.i].is_ascii_digit() {
                self.i += 1;
            }
        }

        Ok(out)
    }

    fn parse_f32(&mut self) -> OrtResult<f32> {
        self.skip_ws();
        if self.eof() {
            return Err(ort_err(ErrorKind::FormatError, "expected number".into()));
        }

        let len = self.b.len();

        // Sign
        let mut neg = false;
        if let Some(c) = self.peek() {
            if c == b'-' {
                neg = true;
                self.i += 1;
            } else if c == b'+' {
                self.i += 1;
            }
        }

        // Mantissa accumulation (up to 9 significant digits)
        let mut mant: u32 = 0;
        let mut mant_digits: i32 = 0;
        let mut ints: i32 = 0;

        // Integer part
        while self.i < len {
            let c = self.b[self.i];
            if c.is_ascii_digit() {
                if mant_digits < 9 {
                    mant = mant.saturating_mul(10).wrapping_add((c - b'0') as u32);
                    mant_digits += 1;
                }
                self.i += 1;
                ints += 1;
            } else {
                break;
            }
        }

        // Fractional part
        let mut frac_any = false;
        if self.peek() == Some(b'.') {
            self.i += 1;
            let start_frac = self.i;
            while self.i < len {
                let c = self.b[self.i];
                if c.is_ascii_digit() {
                    if mant_digits < 9 {
                        mant = mant.saturating_mul(10).wrapping_add((c - b'0') as u32);
                        mant_digits += 1;
                    }
                    self.i += 1;
                } else {
                    break;
                }
            }
            frac_any = self.i > start_frac;
        }

        if ints == 0 && !frac_any {
            return Err(ort_err(ErrorKind::FormatError, "expected number".into()));
        }

        // Exponent part
        let mut exp_part: i32 = 0;
        if let Some(ech) = self.peek()
            && (ech == b'e' || ech == b'E')
        {
            self.i += 1;
            let mut eneg = false;
            if let Some(signch) = self.peek() {
                if signch == b'-' {
                    eneg = true;
                    self.i += 1;
                } else if signch == b'+' {
                    self.i += 1;
                }
            }
            if self.eof() || !self.b[self.i].is_ascii_digit() {
                return Err(ort_err(ErrorKind::FormatError, "expected exponent".into()));
            }
            let mut eacc: i32 = 0;
            while self.i < len {
                let c = self.b[self.i];
                if c.is_ascii_digit() {
                    let d = (c - b'0') as i32;
                    if eacc < 1_000_000_000 / 10 {
                        eacc = eacc * 10 + d;
                    } else {
                        eacc = 1_000_000_000; // clamp large exponents
                    }
                    self.i += 1;
                } else {
                    break;
                }
            }
            exp_part = if eneg { -eacc } else { eacc };
        }

        // Effective base-10 exponent relative to the mantissa we built
        let exp10 = ints - mant_digits + exp_part;

        // Scale using f64 to avoid premature underflow; cast to f32 at the end
        let mut val = mant as f64;

        const POW10_POS: [f64; 39] = [
            1.0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15,
            1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22, 1e23, 1e24, 1e25, 1e26, 1e27, 1e28, 1e29,
            1e30, 1e31, 1e32, 1e33, 1e34, 1e35, 1e36, 1e37, 1e38,
        ];
        const POW10_NEG: [f64; 46] = [
            1.0, 1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-11, 1e-12, 1e-13,
            1e-14, 1e-15, 1e-16, 1e-17, 1e-18, 1e-19, 1e-20, 1e-21, 1e-22, 1e-23, 1e-24, 1e-25,
            1e-26, 1e-27, 1e-28, 1e-29, 1e-30, 1e-31, 1e-32, 1e-33, 1e-34, 1e-35, 1e-36, 1e-37,
            1e-38, 1e-39, 1e-40, 1e-41, 1e-42, 1e-43, 1e-44, 1e-45,
        ];

        if exp10 > 0 {
            let mut e = exp10;
            while e > 0 {
                let chunk = if e > 38 { 38 } else { e } as usize;
                val *= POW10_POS[chunk];
                if !val.is_finite() {
                    return Err(ort_err(ErrorKind::FormatError, "f32 overflow".into()));
                }
                e -= chunk as i32;
            }
        } else if exp10 < 0 {
            let mut e = -exp10;
            while e > 0 {
                let chunk = if e > 45 { 45 } else { e } as usize;
                val *= POW10_NEG[chunk];
                if val == 0.0 {
                    break;
                }
                e -= chunk as i32;
            }
        }

        let mut out = val as f32;
        if !out.is_finite() {
            return Err(ort_err(ErrorKind::FormatError, "f32 overflow".into()));
        }
        if neg {
            out = -out;
        }
        Ok(out)
    }

    pub fn parse_simple_str(&mut self) -> OrtResult<&'a str> {
        self.skip_ws();
        if self.peek() != Some(b'"') {
            let mut msg = "expected opening double quote '\"' got '".to_string();
            // X is better than panic during error message
            msg.push(self.peek().unwrap_or(b'X') as char);
            msg.push('\'');
            return Err(ort_err(ErrorKind::FormatError, msg.into()));
        }
        self.i += 1;
        let start = self.i;
        let len = self.b.len();
        while self.i < len {
            let c = self.b[self.i];
            if c == b'\\' {
                // For maximum speed and simplicity, we reject escapes.
                return Err(ort_err(
                    ErrorKind::FormatError,
                    "string escapes are not supported".into(),
                ));
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
        Err(ort_err(
            ErrorKind::FormatError,
            "unterminated string in parse_simple_str".into(),
        ))
    }

    pub fn parse_string(&mut self) -> OrtResult<String> {
        self.skip_ws();
        if self.peek() != Some(b'"') {
            return Err(ort_err(
                ErrorKind::FormatError,
                ("expected string got: ".to_string() + &String::from_utf8_lossy(&self.b[self.i..]))
                    .into(),
            ));
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
                let s = core::str::from_utf8(&self.b[start..i])
                    .map_err(|_| ort_err(ErrorKind::FormatError, "utf8 error".into()))?;
                self.i = i + 1;
                return Ok(s.to_owned());
            }
            i += 1;
        }
        if !needs_unescape {
            return Err(ort_err(
                ErrorKind::FormatError,
                "unterminated string in parse_string escape scan".into(),
            ));
        }

        // Second pass: build with unescape
        // The "256" should include the biggest single-chunk reasoning item, which
        // depends on the inference server's caching.
        let mut out = String::with_capacity((i - start) + 256);
        let mut seg_start = start;
        while i < len {
            let c = self.b[i];
            if c == b'\\' {
                // push preceding segment
                if i > seg_start {
                    let prev = core::str::from_utf8(&self.b[seg_start..i])
                        .map_err(|_| ort_err(ErrorKind::FormatError, "utf8 error".into()))?;
                    out.push_str(prev);
                }
                i += 1;
                if i >= len {
                    return Err(ort_err(
                        ErrorKind::FormatError,
                        "wrong length parsing escape".into(),
                    ));
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
                            return Err(ort_err(ErrorKind::FormatError, "invalid unicode".into()));
                        }
                    }
                    0 => { // skip null bytes
                    }
                    _ => {
                        //let x_s = crate::utils::num_to_string(x);
                        //crate::utils::print_string(c"Unhandled escape: ", &x_s);
                        return Err(ort_err(ErrorKind::FormatError, "unhandled escape".into()));
                    }
                }
                i += 1;
                seg_start = i;
                continue;
            } else if c == b'"' {
                // end
                if i > seg_start {
                    out.push_str(
                        core::str::from_utf8(&self.b[seg_start..i])
                            .map_err(|_| ort_err(ErrorKind::FormatError, "utf8 error".into()))?,
                    );
                }
                self.i = i + 1;
                return Ok(out);
            } else {
                i += 1;
            }
        }
        Err(ort_err(
            ErrorKind::FormatError,
            "unterminated string in parse_string".into(),
        ))
    }

    // Parses \uXXXX (with surrogate-pair handling). Input index points at first hex digit after 'u'.
    fn parse_u_escape(&self, i: usize) -> OrtResult<(u32, usize)> {
        fn hex4(bytes: &[u8], i: usize) -> OrtResult<(u16, usize)> {
            let end = i + 4;
            if end > bytes.len() {
                return Err(ort_err(ErrorKind::FormatError, "short \\u".into()));
            }
            let mut v: u16 = 0;
            for b in bytes.iter().take(end).skip(i) {
                v = (v << 4) | hex_val(*b)?;
            }
            Ok((v, end))
        }
        fn hex_val(b: u8) -> OrtResult<u16> {
            match b {
                b'0'..=b'9' => Ok((b - b'0') as u16),
                b'a'..=b'f' => Ok((b - b'a' + 10) as u16),
                b'A'..=b'F' => Ok((b - b'A' + 10) as u16),
                _ => Err(ort_err(ErrorKind::FormatError, "bad hex".into())),
            }
        }

        let (first, i2) = hex4(self.b, i)?;
        let cp = first as u32;

        // Surrogate pair handling
        if (0xD800..=0xDBFF).contains(&first) {
            // Expect \uXXXX next
            if i2 + 2 > self.b.len() || self.b[i2] != b'\\' || self.b[i2 + 1] != b'u' {
                return Err(ort_err(
                    ErrorKind::FormatError,
                    "missing low surrogate".into(),
                ));
            }
            let (second, i3) = hex4(self.b, i2 + 2)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err(ort_err(
                    ErrorKind::FormatError,
                    "invalid low surrogate".into(),
                ));
            }
            let high = (first as u32) - 0xD800;
            let low = (second as u32) - 0xDC00;
            let code = 0x10000 + ((high << 10) | low);
            Ok((code, i3))
        } else if (0xDC00..=0xDFFF).contains(&first) {
            Err(ort_err(
                ErrorKind::FormatError,
                "unpaired low surrogate".into(),
            ))
        } else {
            Ok((cp, i2))
        }
    }

    // Returns a slice of the next JSON value and advances past it.
    pub fn value_slice(&mut self) -> OrtResult<&'a str> {
        self.skip_ws();
        let start = self.i;
        let end = self.find_value_end().context("value_slice")?;
        let out = &self.s[start..end];
        self.i = end;
        Ok(out)
    }

    // Skips the next JSON value (string/number/boolean/null/object/array).
    pub fn skip_value(&mut self) -> OrtResult<()> {
        let _ = self.value_slice().context("skip_value")?;
        Ok(())
    }

    fn find_value_end(&mut self) -> OrtResult<usize> {
        if self.eof() {
            return Err(ort_err(ErrorKind::FormatError, "unexpected end".into()));
        }
        match self.b[self.i] {
            b'"' => self
                .scan_string_end()
                .context("find_value_end double quote"),
            b'{' => self
                .scan_brace_block(b'{', b'}')
                .context("find_value_end curly"),
            b'[' => self
                .scan_brace_block(b'[', b']')
                .context("find_value_end square"),
            b't' => {
                if self.starts_with_bytes(b"true") {
                    Ok(self.i + 4)
                } else {
                    Err(ort_err(ErrorKind::FormatError, "bad literal".into()))
                }
            }
            b'f' => {
                if self.starts_with_bytes(b"false") {
                    Ok(self.i + 5)
                } else {
                    Err(ort_err(ErrorKind::FormatError, "bad literal".into()))
                }
            }
            b'n' => {
                if self.starts_with_bytes(b"null") {
                    Ok(self.i + 4)
                } else {
                    Err(ort_err(ErrorKind::FormatError, "bad literal".into()))
                }
            }
            b'-' | b'0'..=b'9' => self.scan_number_end(),
            _t => {
                //let t_str = crate::utils::num_to_string(t as usize);
                //crate::utils::print_string(c"unexpected token: ", &t_str);
                Err(ort_err(
                    ErrorKind::FormatError,
                    "unexpected token in find_value_end".into(),
                ))
            }
        }
    }

    /// Skip a string by moving i to the next char after the closing quote.
    fn scan_string_end(&self) -> OrtResult<usize> {
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
        let len_string = utils::num_to_string(len);
        let msg = "unterminated string in scan_string_end after bytes: ".to_string() + &len_string;
        Err(ort_err(ErrorKind::FormatError, msg.into()))
    }

    fn scan_brace_block(&self, open: u8, close: u8) -> OrtResult<usize> {
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
                i = p.scan_string_end().context("scan_brace_block")?; // returns position after closing "
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
        Err(ort_err(
            ErrorKind::FormatError,
            "unterminated structure in scan_brace_block".into(),
        ))
    }

    fn scan_number_end(&self) -> OrtResult<usize> {
        let len = self.b.len();
        let mut i = self.i;

        if self.b[i] == b'-' {
            i += 1;
            if i >= len {
                return Err(ort_err(ErrorKind::FormatError, "bad number".into()));
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
            _ => return Err(ort_err(ErrorKind::FormatError, "bad number".into())),
        }

        // frac
        if i < len && self.b[i] == b'.' {
            i += 1;
            if i >= len || !self.b[i].is_ascii_digit() {
                return Err(ort_err(ErrorKind::FormatError, "bad number".into()));
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
                return Err(ort_err(ErrorKind::FormatError, "bad number".into()));
            }
            while i < len && self.b[i].is_ascii_digit() {
                i += 1;
            }
        }

        Ok(i)
    }
}
