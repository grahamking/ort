//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King

use core::str::FromStr;

extern crate alloc;
use alloc::borrow::{Cow, ToOwned};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::common::config;
use crate::common::data::{
    Content, Function, PromptFile, PromptFileKind, Tool, ToolCall, ToolParameter,
};
use crate::common::tools::ReadTool;
use crate::{
    ChatCompletionsResponse, Choice, LastData, Message, Priority, PromptOpts, ReasoningConfig,
    ReasoningEffort, Role, Usage,
};

impl ChatCompletionsResponse {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_simple_string("provider"),
            JsonField::new_simple_string("model"),
            JsonField::new_vec_raw("choices"),
            JsonField::new_raw("usage"),
        ];
        autoparser(json, &mut fields)?;

        let provider = fields[0].get_string();
        let model = fields[1].get_string();
        let mut choices = vec![];
        if let Some(v) = fields[2].get_vec_raw() {
            for c in v {
                choices.push(Choice::from_json(&c)?);
            }
        }

        let usage = fields[3]
            .get_raw()
            .as_deref()
            .map(Usage::from_json)
            .transpose()?;

        Ok(ChatCompletionsResponse {
            provider,
            model,
            choices,
            usage,
        })
    }
}

impl Choice {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_raw("delta"),
            JsonField::new_simple_string("finish_reason"),
        ];
        autoparser(json, &mut fields)?;
        let delta_json = fields[0].get_raw().expect("Missing delta in message");
        let delta = Message::from_json(&delta_json)?;
        let finish_reason = fields[1].get_string();

        Ok(Choice {
            delta,
            finish_reason,
        })
    }
}

impl ToolCall {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_int("index"),
            JsonField::new_simple_string("id"),
            JsonField::new_raw("function"),
        ];
        autoparser(json, &mut fields)?;

        let index = fields[0].get_int().unwrap_or_default();
        let id = fields[1].get_string();
        let function_json = fields[2].get_raw().expect("Missing function in tool call");
        let function = Function::from_json(&function_json)?;
        Ok(ToolCall {
            index,
            id,
            function,
        })
    }
}

impl Function {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_simple_string("name"),
            JsonField::new_string("arguments"),
        ];
        autoparser(json, &mut fields)?;
        Ok(Function {
            name: fields[0].get_string().unwrap_or_default(),
            arguments: fields[1].get_string().unwrap_or_default(),
        })
    }
}

impl Usage {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [JsonField::new_float("cost")];
        autoparser(json, &mut fields)?;
        Ok(Usage {
            cost: fields[0].get_float().unwrap_or_default(),
        })
    }
}

impl LastData {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        if json.is_empty() {
            return Err(
                "Cannot continue, last-<$TMUX_PANE>.json file is empty. Usually that mains previous run failed.".into(),
            );
        }

        let mut fields = [
            JsonField::new_raw("opts"),
            JsonField::new_vec_raw("messages"),
            JsonField::new_vec_raw("tools"),
        ];
        autoparser(json, &mut fields)?;

        let opts = fields[0]
            .get_raw()
            .as_deref()
            .map(PromptOpts::from_json)
            .transpose()?;

        let mut messages = vec![];
        if let Some(msg_vec) = fields[1].get_vec_raw() {
            for m in msg_vec {
                messages.push(Message::from_json(&m)?);
            }
        }

        let mut tools = vec![];
        if let Some(tools_vec) = fields[2].get_vec_raw() {
            for t in tools_vec {
                tools.push(Tool::from_json(&t)?);
            }
        }

        Ok(LastData {
            opts: opts.expect("Missing prompt opts"),
            messages,
            tools,
        })
    }
}

impl Message {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_simple_string("role"),
            JsonField::new_raw("content"),
            JsonField::new_simple_string("reasoning"),
            JsonField::new_vec_raw("tool_calls"),
        ];
        autoparser(json, &mut fields)?;

        let role = fields[0]
            .get_raw()
            .as_deref()
            .map(Role::from_str)
            .transpose()?;
        let reasoning = fields[2].get_string();

        let mut tool_calls = vec![];
        if let Some(tool_calls_str_vec) = fields[3].get_vec_raw() {
            for t in tool_calls_str_vec {
                tool_calls.push(ToolCall::from_json(&t)?);
            }
        }

        // Content can be a string or an array, so do extra parsing
        let mut content = vec![];
        if let Some(content_str) = fields[1].get_raw() {
            let mut p = Parser::new(&content_str);
            if p.peek_is_null() {
                p.skip_null()?;
            } else if p.peek() == Some(b'[') {
                p.expect(b'[')?;
                p.skip_ws();
                if !p.try_consume(b']') {
                    loop {
                        let j = p.value_slice()?;
                        content.push(Content::from_json(j)?);
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
            } else {
                content.push(Content::Text(p.parse_string()?));
            }
        }

        Ok(Message::with_content(
            // NVIDIA doesn't always send it. sus.
            role.unwrap_or(Role::Assistant),
            content,
            reasoning,
            tool_calls,
        ))
    }
}

impl Content {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_simple_string("type"),
            JsonField::new_string("text"),
            JsonField::new_raw("image_url"),
            JsonField::new_raw("file"),
        ];
        autoparser(json, &mut fields)?;

        let kind = fields[0].get_string();
        let text = fields[1].get_string();

        let mut base64_data = None;
        let mut mime_type = None;
        let mut image_url = None;
        if let Some(image_url_str) = fields[2].get_raw() {
            if image_url_str.starts_with("http") {
                image_url = Some(image_url_str);
            } else {
                let (base64, mt) = parse_image_url(&image_url_str)?;
                base64_data = Some(base64);
                mime_type = Some(mt);
            }
        }

        let file = fields[3]
            .get_raw()
            .as_deref()
            .map(PromptFile::from_json)
            .transpose()?;

        match kind.as_deref() {
            Some("text") => Ok(Content::Text(text.ok_or("missing text")?)),
            Some("image_url") => {
                if let Some(image_url) = image_url {
                    Ok(Content::ImageUrl(image_url.to_string()))
                } else {
                    Ok(Content::Image {
                        base64: base64_data.ok_or("missing image_url")?,
                        mime_type: mime_type.unwrap(),
                    })
                }
            }
            Some("file") => Ok(Content::File(file.ok_or("missing file")?)),
            Some(other) => Err("unsupported content type: ".to_string() + other),
            None => Err("missing content type".to_string()),
        }
    }
}

impl PromptFile {
    fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_string("filename"),
            JsonField::new_raw("file_data"),
        ];
        autoparser(json, &mut fields)?;

        let filename = fields[0].get_string();

        let base64 = fields[1].get_raw().map(|data| {
            data.strip_prefix("data:application/pdf;base64,")
                .unwrap_or(data.as_str())
                .to_string()
        });

        Ok(PromptFile::from_parts(
            PromptFileKind::File,
            filename.ok_or("missing filename")?,
            base64.ok_or("missing file_data")?,
        ))
    }
}

/// Returns (base64_data, mime_type)
fn parse_image_url(json: &str) -> Result<(String, &'static str), String> {
    let mut fields = [JsonField::new_string("url")];
    autoparser(json, &mut fields)?;

    let url_str = fields[0].get_string().expect("Missing image URL");
    if url_str.starts_with("data:image/jpeg") {
        Ok((
            url_str
                .strip_prefix("data:image/jpeg;base64,")
                .unwrap()
                .to_string(),
            "image/jpeg",
        ))
    } else if url_str.starts_with("data:image/png") {
        Ok((
            url_str
                .strip_prefix("data:image/png;base64,")
                .unwrap()
                .to_string(),
            "image/png",
        ))
    } else {
        Err("Invalid mime type in saved image_url".to_string())
    }
}

impl ReasoningConfig {
    pub fn from_json(json: &str) -> Result<ReasoningConfig, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_bool("enabled"),
            JsonField::new_string("effort"),
            JsonField::new_int("tokens"),
        ];
        autoparser(json, &mut fields)?;

        let mut effort = None;
        if let Some(v) = fields[1].get_string() {
            let e = if v.eq_ignore_ascii_case("none") {
                ReasoningEffort::None
            } else if v.eq_ignore_ascii_case("low") {
                ReasoningEffort::Low
            } else if v.eq_ignore_ascii_case("medium") {
                ReasoningEffort::Medium
            } else if v.eq_ignore_ascii_case("high") {
                ReasoningEffort::High
            } else if v.eq_ignore_ascii_case("xhigh") {
                ReasoningEffort::XHigh
            } else {
                return Err("invalid effort".into());
            };
            effort = Some(e);
        }

        Ok(ReasoningConfig {
            effort,
            enabled: fields[0]
                .get_bool()
                .ok_or("missing required field: enabled")?,
            tokens: fields[2].get_int(),
        })
    }
}

impl PromptOpts {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_string("prompt"),
            JsonField::new_simple_string("model"),
            JsonField::new_simple_string("provider"),
            JsonField::new_string("system"),
            JsonField::new_simple_string("priority"),
            JsonField::new_raw("reasoning"),
            JsonField::new_bool("show_reasoning"),
            JsonField::new_bool("quiet"),
            JsonField::new_bool("merge_config"),
        ];
        autoparser(json, &mut fields)?;

        let priority = fields[4]
            .get_string()
            .as_deref()
            .map(Priority::from_str)
            .transpose()?;
        let reasoning = fields[5]
            .get_raw()
            .as_deref()
            .map(ReasoningConfig::from_json)
            .transpose()?;

        Ok(PromptOpts {
            prompt: fields[0].get_string(),
            models: fields[1].get_string().map(|m| vec![m]).unwrap_or_default(),
            provider: fields[2].get_string(),
            system: fields[3].get_string(),
            priority,
            reasoning,
            show_reasoning: fields[6].get_bool(),
            quiet: fields[7].get_bool(),
            merge_config: fields[8].get_bool().unwrap_or(true),
            prompt_filename: None,
            // TODO: store files in last json, so resume works with files
            files: vec![],
        })
    }
}

impl Tool {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut p = Parser::new(json);
        p.skip_ws();

        // Skip the preamble:
        // {"type": "function", "function": {
        p.expect(b'{')?;
        p.skip_ws();
        p.skip_value()?; // skip "type"
        p.expect(b':')?;
        p.skip_ws();
        p.skip_value()?; // skip "function" from type:function
        p.expect(b',')?;
        p.skip_ws();
        p.skip_value()?; // skip "function" as key
        p.expect(b':')?;
        p.skip_ws();
        p.expect(b'{')?;

        let mut name = String::new();
        let mut description = String::new();
        let mut parameters = vec![];
        let mut required_parameters = vec![];

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            let key = p
                .parse_simple_str()
                .map_err(|err| "Message parsing key: ".to_string() + err)?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key {
                "name" => {
                    name = p.parse_simple_str()?.to_string();
                }
                "description" => {
                    description = p.parse_string()?;
                }
                "parameters" => {
                    // Skip
                    // {"type": "object", "properties": {
                    p.expect(b'{')?;
                    p.skip_value()?; // skip "type"
                    p.expect(b':')?;
                    p.skip_ws();
                    p.skip_value()?; // skip "object"
                    p.expect(b',')?;
                    p.skip_ws();
                    p.skip_value()?; // skip "properties"
                    p.expect(b':')?;
                    p.skip_ws();
                    p.expect(b'{')?;

                    let param_name = p.parse_simple_str()?.to_string();
                    p.skip_ws();
                    p.expect(b':')?;
                    p.skip_ws();
                    p.expect(b'{')?;
                    p.skip_ws();

                    let mut param_type = None;
                    let mut description = None;
                    loop {
                        let param_key = p.parse_simple_str()?;
                        p.skip_ws();
                        p.expect(b':')?;
                        p.skip_ws();

                        match param_key {
                            "type" => {
                                param_type = Some(p.parse_simple_str()?.to_string());
                            }
                            "description" => {
                                description = Some(p.parse_simple_str()?.to_string());
                            }
                            _ => {}
                        }
                        p.skip_ws();
                        if p.try_consume(b',') {
                            continue;
                        } else {
                            p.expect(b'}')?;
                            break;
                        }
                    }

                    // TODO: description can be optional. and no unwrap
                    parameters.push(ToolParameter {
                        name: param_name,
                        param_type: param_type.unwrap(),
                        description: description.unwrap(),
                    });
                }
                "required" => {
                    p.expect(b'[')?;
                    p.skip_ws();
                    if !p.try_consume(b']') {
                        loop {
                            let param_name = p.parse_simple_str()?;
                            required_parameters.push(param_name.to_string());
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
                }
                _ => {
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

        Ok(Tool {
            name,
            description,
            parameters,
            required_parameters,
        })
    }
}

impl config::ConfigFile {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_raw("settings"),
            JsonField::new_vec_raw("keys"),
            JsonField::new_raw("prompt_opts"),
        ];
        autoparser(json, &mut fields)?;

        let settings = fields[0]
            .get_raw()
            .as_deref()
            .map(config::Settings::from_json)
            .transpose()?;

        let mut keys = vec![];
        if let Some(keys_str) = fields[1].get_vec_raw() {
            for k in keys_str {
                keys.push(config::ApiKey::from_json(&k)?);
            }
        }

        let prompt_opts = fields[2]
            .get_raw()
            .as_deref()
            .map(PromptOpts::from_json)
            .transpose()?;

        Ok(config::ConfigFile {
            settings,
            keys,
            prompt_opts,
        })
    }
}

impl config::Settings {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
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
                .map_err(|err| "Settings parsing key: ".to_string() + err)?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key {
                "save_to_file" => {
                    if save_to_file.is_some() {
                        return Err("duplicate field: save_to_file".into());
                    }
                    if p.peek_is_null() {
                        p.skip_null()?;
                        save_to_file = None;
                    } else {
                        save_to_file = Some(p.parse_bool()?);
                    }
                }
                "dns" => {
                    if !dns.is_empty() {
                        return Err("duplicate field: dns".into());
                    }
                    if !p.try_consume(b'[') {
                        return Err("dns: Expected array".into());
                    }
                    loop {
                        let addr = p.parse_simple_str()?;
                        dns.push(addr.to_string());
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

            // If neither comma nor closing brace, it's malformed.
            if !p.eof() {
                return Err("expected ',' or '}'".into());
            } else {
                return Err("unexpected end of input".into());
            }
        }

        let default = config::Settings::default();
        Ok(config::Settings {
            save_to_file: save_to_file.unwrap_or(default.save_to_file),
            dns,
        })
    }
}

impl config::ApiKey {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_simple_string("name"),
            JsonField::new_string("value"),
        ];
        autoparser(json, &mut fields)?;
        Ok(config::ApiKey::new(
            fields[0].get_string().expect("Missing ApiKey name"),
            fields[1].get_string().expect("Missing ApiKey value"),
        ))
    }
}

impl ReadTool {
    // Example JSON: { "path": "README.md", offset: 100, limit: 500 }
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_simple_string("path"),
            JsonField::new_int("offset"),
            JsonField::new_int("limit"),
        ];
        autoparser(json, &mut fields)?;
        Ok(ReadTool {
            path: fields[0].get_string().expect("Missing ReadTool path"),
            offset: fields[1].get_int(),
            limit: fields[2].get_int(),
        })
    }
}

// --------------------------------------------

struct JsonField {
    name: &'static str,
    value: JsonValue,
}

impl JsonField {
    fn new_simple_string(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::SimpleString(None),
        }
    }

    fn new_string(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::String(None),
        }
    }

    fn new_int(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::Int(None),
        }
    }

    fn new_float(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::Float(None),
        }
    }

    fn new_bool(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::Bool(None),
        }
    }

    fn new_raw(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::Raw(None),
        }
    }

    fn new_vec_raw(name: &'static str) -> JsonField {
        JsonField {
            name,
            value: JsonValue::VecRaw(None),
        }
    }

    fn parse(&mut self, p: &mut Parser) -> Result<(), Cow<'static, str>> {
        match &mut self.value {
            JsonValue::SimpleString(inner) => {
                let s = p
                    .parse_simple_str()
                    .map_err(|err| "Parsing field: ".to_string() + err)?;
                inner.replace(s.to_string());
            }
            JsonValue::String(inner) => {
                let s = p
                    .parse_string()
                    .map_err(|err| "Parsing field: ".to_string() + &err)?;
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
                let v = p.value_slice()?;
                // figure out lifetimes and keep as &str
                inner.replace(v.to_string());
            }
            JsonValue::VecRaw(inner) => {
                if !p.try_consume(b'[') {
                    return Err("Expected array".into());
                }
                p.skip_ws();
                let mut v = vec![];
                // If the array isn't empty..
                if !p.try_consume(b']') {
                    loop {
                        let j = p.value_slice()?;
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

    fn get_string(&mut self) -> Option<String> {
        match &mut self.value {
            JsonValue::String(s) | JsonValue::SimpleString(s) => s.take(),
            _ => None,
        }
    }

    fn get_int(&mut self) -> Option<u32> {
        match &mut self.value {
            JsonValue::Int(i) => i.take(),
            _ => None,
        }
    }

    fn get_float(&mut self) -> Option<f32> {
        match &mut self.value {
            JsonValue::Float(f) => f.take(),
            _ => None,
        }
    }

    fn get_bool(&mut self) -> Option<bool> {
        match &mut self.value {
            JsonValue::Bool(b) => b.take(),
            _ => None,
        }
    }

    fn get_raw(&mut self) -> Option<String> {
        match &mut self.value {
            JsonValue::Raw(s) => s.take(),
            _ => None,
        }
    }

    fn get_vec_raw(&mut self) -> Option<Vec<String>> {
        match &mut self.value {
            JsonValue::VecRaw(v) => core::mem::take(v),
            _ => None,
        }
    }

    fn is_empty(&self) -> bool {
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

enum JsonValue {
    /// A string with no escapes. Faster to parse.
    SimpleString(Option<String>),

    /// Any string
    String(Option<String>),

    Int(Option<u32>),

    Float(Option<f32>),

    Bool(Option<bool>),

    /// Something the caller will parse themselves
    Raw(Option<String>),

    VecRaw(Option<Vec<String>>),
}

impl JsonValue {}

fn autoparser(json: &str, fields: &mut [JsonField]) -> Result<(), Cow<'static, str>> {
    let mut p = Parser::new(json);
    p.skip_ws();
    p.expect(b'{')?;

    loop {
        p.skip_ws();
        if p.try_consume(b'}') {
            break;
        }

        let key = p
            .parse_simple_str()
            .map_err(|err| "parsing key: ".to_string() + err)?;
        p.skip_ws();
        p.expect(b':')?;
        p.skip_ws();

        let mut is_parsed = false;
        for field in fields.iter_mut() {
            if field.name == key {
                if !field.is_empty() {
                    return Err(("duplicate field: ".to_string() + field.name).into());
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
        if !is_parsed {
            p.skip_value()?;
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
        return Err("trailing characters after JSON object".into());
    }

    /*
    crate::utils::print_string(c"-- After", "");
    for (name, value) in fields.iter() {
        crate::utils::print_string(c"\nName: ", name);
        match value {
            JsonValue::String(Some(s)) | JsonValue::SimpleString(Some(s)) => {
                crate::utils::print_string(c"\nValue: ", s);
            }
            _ => {}
        }
    }
    crate::utils::print_string(c"\n-- End After", "");
    */

    Ok(())
}

// --------------------------------------------

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

    fn skip_null(&mut self) -> Result<(), &'static str> {
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

    fn parse_bool(&mut self) -> Result<bool, String> {
        self.skip_ws();
        if self.starts_with_bytes(b"true") {
            self.i += 4;
            Ok(true)
        } else if self.starts_with_bytes(b"false") {
            self.i += 5;
            Ok(false)
        } else {
            Err("Expected boolean, got: ".to_string() + &String::from_utf8_lossy(&self.b[self.i..]))
        }
    }

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

    fn parse_f32(&mut self) -> Result<f32, &'static str> {
        self.skip_ws();
        if self.eof() {
            return Err("expected number");
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
            return Err("expected number");
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
                return Err("expected exponent");
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
                    return Err("f32 overflow");
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
            return Err("f32 overflow");
        }
        if neg {
            out = -out;
        }
        Ok(out)
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
        Err("unterminated string in parse_simple_str")
    }

    fn parse_string(&mut self) -> Result<String, Cow<'static, str>> {
        self.skip_ws();
        if self.peek() != Some(b'"') {
            return Err(("expected string got: ".to_string()
                + &String::from_utf8_lossy(&self.b[self.i..]))
                .into());
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
            return Err("unterminated string in parse_string escape scan".into());
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
                    let prev =
                        core::str::from_utf8(&self.b[seg_start..i]).map_err(|_| "utf8 error")?;
                    out.push_str(prev);
                }
                i += 1;
                if i >= len {
                    return Err("bad escape".into());
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
                            return Err("invalid unicode".into());
                        }
                    }
                    _ => return Err("bad escape".into()),
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
        Err("unterminated string in parse_string".into())
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
            _t => {
                //let t_str = crate::utils::num_to_string(t as usize);
                //crate::utils::print_string(c"unexpected token: ", &t_str);
                Err("unexpected token in find_value_end")
            }
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
        Err("unterminated string in scan_string_end")
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
    extern crate alloc;
    use alloc::string::ToString;

    use super::*;
    use crate::LastData;

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
        assert_eq!(opts.models, vec!["google/gemma-3n-e4b-it:free"]);
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
        assert_eq!(opts.models, vec!["openai/gpt-5"]);
        assert!(opts.reasoning.as_ref().unwrap().enabled);
        assert_eq!(
            opts.reasoning.as_ref().unwrap().effort,
            Some(ReasoningEffort::High)
        );
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

    #[test]
    fn test_usage() {
        let s = r#"{"prompt_tokens":42,"completion_tokens":2,"total_tokens":44,"cost":0.0534,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0,"upstream_inference_completions_cost":0},"completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}"#;
        let usage = Usage::from_json(s).unwrap();
        assert_eq!(usage.cost, 0.0534);
    }

    #[test]
    fn test_choice() {
        let s = r#"{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}"#;
        let choice = Choice::from_json(s).unwrap();
        assert_eq!(choice.delta.text(), Some("Hello"));
    }

    #[test]
    fn test_chat_completions_response_simple() {
        let arr = [
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}]}"#,
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}],"usage":{"prompt_tokens":42,"completion_tokens":2,"total_tokens":44,"cost":0,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0,"upstream_inference_completions_cost":0},"completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}}"#,
        ];
        for a in arr {
            let ccr = ChatCompletionsResponse::from_json(a).unwrap();
            assert_eq!(ccr.provider.as_deref(), Some("Meta"));
            assert_eq!(
                ccr.model.as_deref(),
                Some("meta-llama/llama-3.3-8b-instruct:free")
            );
            assert_eq!(ccr.choices.len(), 1);
        }
    }

    #[test]
    fn test_chat_completions_response_more() {
        let arr = [
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"Rea","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"l","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":" Madrid, 14 times.","reasoning":null,"reasoning_details":[]},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}],"usage":{"prompt_tokens":33,"completion_tokens":8,"total_tokens":41,"cost":0.0000310365,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0.00001815,"upstream_inference_completions_cost":0.0000132},"completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}}"#,
        ];
        for a in arr {
            let ccr = ChatCompletionsResponse::from_json(a).unwrap();
            assert_eq!(ccr.provider.as_deref(), Some("WandB"));
            assert_eq!(ccr.model.as_deref(), Some("deepseek/deepseek-chat-v3.1"));
            assert_eq!(ccr.choices.len(), 1);
        }
    }

    // Various null fields, including inside the message, and usage.
    #[test]
    fn test_nvidia_misc() {
        let s = r#"{"id":"8f20d6699e194a0abed38c671384d32d","object":"chat.completion.chunk","created":1770582573,"model":"qwen/qwen3-next-80b-a3b-instruct","choices":[{"index":0,"delta":{"role":null,"content":"Ta","reasoning_content":null,"tool_calls":null},"logprobs":null,"finish_reason":null,"matched_stop":null}],"usage":null}"#;
        let ccr = ChatCompletionsResponse::from_json(s).unwrap();
        assert_eq!(ccr.choices[0].delta.text(), Some("Ta"));
    }

    #[test]
    fn message_content_array() {
        let s = r#"{"role":"user","content":[{"type":"text","text":"Hello"},{"type":"text","text":" there"}]}"#;
        let msg = Message::from_json(s).unwrap();
        assert_eq!(msg.content.len(), 2);
        assert_eq!(msg.content[0].text(), Some("Hello"));
        assert_eq!(msg.content[1].text(), Some(" there"));
    }

    #[test]
    fn api_key() {
        let s = r#"{"name":"openrouter","value":"sk-or-v1-a123b456c789d012a345b8032470394876576573242374098174093274abcdef"}"#;
        let got = config::ApiKey::from_json(s).unwrap();
        let expect = config::ApiKey::new(
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
        let settings = config::Settings::from_json(s).unwrap();
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
        let cfg = config::ConfigFile::from_json(s).unwrap();
        assert_eq!(cfg.keys.len(), 1);
        assert!(cfg.settings.is_some());
        assert!(cfg.prompt_opts.is_some());
    }
}
