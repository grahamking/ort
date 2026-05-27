//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::borrow::Cow;
use alloc::string::String;

use crate::common::json_parser::{JsonField, autoparser};

pub struct ReadTool {
    /// Path to the file to read (relative or absolute)
    pub path: String,
    /// Line number to start reading from
    #[allow(unused)]
    pub offset: Option<u32>,
    /// Maximum number of lines to read
    #[allow(unused)]
    pub limit: Option<u32>,
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

pub struct BashTool {
    pub command: String,
}

impl BashTool {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [JsonField::new_string("command")];
        autoparser(json, &mut fields)?;
        Ok(BashTool {
            command: fields[0].get_string().expect("Missing BashTool command"),
        })
    }
}

pub struct WriteTool {
    pub path: String,
    pub content: String,
}

impl WriteTool {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_simple_string("path"),
            JsonField::new_string("content"),
        ];
        autoparser(json, &mut fields)?;
        Ok(WriteTool {
            path: fields[0].get_string().expect("Missing WriteTool path"),
            content: fields[1].get_string().expect("Missing WriteTool content"),
        })
    }
}
