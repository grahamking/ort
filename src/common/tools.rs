//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::borrow::Cow;
use alloc::string::String;

use crate::common::{
    data::{Tool, ToolParameter},
    json_parser::{JsonField, autoparser},
};

pub const ALL_TOOLS: &[&Tool] = &[&TOOL_READ, &TOOL_BASH, &TOOL_WRITE, &TOOL_EDIT];

const TOOL_READ: Tool = Tool {
    name: "read",
    description: "Read the contents of a text file.",
    parameters: &[
        ToolParameter {
            name: "path",
            param_type: "string",
            description: "Path to the file to read (relative or absolute)",
        },
        ToolParameter {
            name: "offset",
            param_type: "number",
            description: "Line number to start reading from (1-indexed)",
        },
        ToolParameter {
            name: "limit",
            param_type: "number",
            description: "Maximum number of lines to read",
        },
    ],
    required_parameters: &["path"],
};

const TOOL_BASH: Tool = Tool {
    name: "bash",
    description: "Execute a bash command in the current working directory. Returns stdout and stderr.",
    parameters: &[ToolParameter {
        name: "command",
        param_type: "string",
        description: "Bash command to execute",
    }],
    required_parameters: &["command"],
};

const TOOL_WRITE: Tool = Tool {
    name: "write",
    description: "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories. Use only for new files or complete rewrites.",
    parameters: &[
        ToolParameter {
            name: "path",
            param_type: "string",
            description: "Path to the file to write (relative or absolute)",
        },
        ToolParameter {
            name: "content",
            param_type: "string",
            description: "Content to write to the file",
        },
    ],
    required_parameters: &["path", "content"],
};

const TOOL_EDIT: Tool = Tool {
    name: "edit",
    description: "Edit a file by replacing an exact old_text span with new_text. Fails if old_text is not found exactly once unless replace_all is true or expected_occurrences is provi
ded.",
    parameters: &[
        ToolParameter {
            name: "path",
            param_type: "string",
            description: "Path to the file to edit.",
        },
        ToolParameter {
            name: "old_text",
            param_type: "string",
            description: "Exact text to find in the file.",
        },
        ToolParameter {
            name: "new_text",
            param_type: "string",
            description: "Replacement text.",
        },
        ToolParameter {
            name: "replace_all",
            param_type: "boolean",
            description: "If true, replace all matches of old_text. Defaults to false, meaning only replace the first match.",
        },
    ],
    required_parameters: &["path", "old_text", "new_text"],
};

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

pub struct EditTool {
    pub path: String,
    pub old_text: String,
    pub new_text: String,
    pub replace_all: bool,
}

impl EditTool {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        // Example JSON:
        // { "path": "LICENSE",
        //   "old_text": "Copyright (c) 2025 Graham King",
        //   "new_text": "Copyright (c) 2025, 2026 Graham King"
        // }
        let mut fields = [
            JsonField::new_simple_string("path"),
            JsonField::new_string("old_text"),
            JsonField::new_string("new_text"),
            JsonField::new_bool("replace_all"),
        ];
        autoparser(json, &mut fields)?;
        Ok(EditTool {
            path: fields[0].get_string().expect("Missing EditTool path"),
            old_text: fields[1].get_string().expect("Missing EditTool old_text"),
            new_text: fields[2].get_string().expect("Missing EditTool new_text"),
            replace_all: fields[3].get_bool().unwrap_or(false),
        })
    }
}
