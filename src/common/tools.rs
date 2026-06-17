//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::common::data::{Function, ToolDisplay};
use crate::{ErrorKind, ort_error};
use crate::{
    OrtResult, Write,
    common::{
        data::{Tool, ToolParameter},
        file::File,
        json_parser::{JsonField, autoparser},
    },
    syscall::system,
    utils,
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

pub fn parse_function(f: &Function) -> OrtResult<Box<dyn ActiveTool>> {
    match f.name.as_ref() {
        "read" => {
            let t = ReadTool::from_json(&f.arguments).map_err(|_err| {
                ort_error(
                    ErrorKind::ParsingToolCallParams,
                    "Parsing read tool params JSON",
                )
            })?;
            Ok(Box::new(t))
        }
        "bash" => {
            let t = BashTool::from_json(&f.arguments).map_err(|_err| {
                ort_error(
                    ErrorKind::ParsingToolCallParams,
                    "Parsing bash tool params JSON",
                )
            })?;
            Ok(Box::new(t))
        }
        "write" => {
            let t = WriteTool::from_json(&f.arguments).map_err(|_err| {
                ort_error(
                    ErrorKind::ParsingToolCallParams,
                    "Parsing write tool params JSON",
                )
            })?;
            Ok(Box::new(t))
        }
        "edit" => {
            let t = EditTool::from_json(&f.arguments).map_err(|_err| {
                ort_error(
                    ErrorKind::ParsingToolCallParams,
                    "Parsing edit tool params JSON",
                )
            })?;
            Ok(Box::new(t))
        }
        _ => Err(ort_error(ErrorKind::ToolDoesNotExist, "")),
    }
}

pub trait ActiveTool {
    /// Run this tool.
    /// On success return Ok(success(..)) which generates the JSON for the model.
    /// On error raise an OrtResult::Err which the caller will convert for the model.
    fn run(&self) -> OrtResult<String>;

    /// How this tool call should be presented to the user.
    fn display(&self) -> ToolDisplay;
}

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

impl ActiveTool for ReadTool {
    fn run(&self) -> OrtResult<String> {
        let content = match utils::filename_read_to_string(&self.path) {
            Ok(content) => content,
            // Return the string error so the model sees it.
            Err("NOT FOUND") => "No such file or directory: ".to_string() + &self.path,
            Err(s) => "Tool call error ".to_string() + s + ": " + &self.path,
        };
        // Ideally limit would limit the original read, so we don't get whole file in memory
        let offset = self.offset.map_or(0, |offset| offset as usize);
        let limit = self.limit.map_or(usize::MAX, |limit| limit as usize);
        let content_lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
        let num_lines = content_lines.len();
        Ok(success(
            &[("lines", num_lines)],
            &[("path", &self.path), ("output", &content_lines.join("\n"))],
        ))
    }

    fn display(&self) -> ToolDisplay {
        ToolDisplay {
            name: "Read ",
            arguments: self.path.clone(),
        }
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

impl ActiveTool for BashTool {
    fn run(&self) -> OrtResult<String> {
        let output = system(&self.command)?;
        Ok(success(
            &[("exit_code", output.exit_code as usize)],
            &[("stdout", &output.stdout), ("stderr", &output.stderr)],
        ))
    }

    fn display(&self) -> ToolDisplay {
        ToolDisplay {
            name: "Bash ",
            arguments: self.command.clone(),
        }
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

impl ActiveTool for WriteTool {
    fn run(&self) -> OrtResult<String> {
        if let Some(idx) = self.path.rfind('/') {
            let dir_path = &self.path[..idx];
            // TODO does not create ancestors
            utils::ensure_dir_exists(dir_path);
        }

        // Write the file
        let mut c_path = [0u8; 128];
        let end = self.path.len();
        c_path[..end].copy_from_slice(self.path.as_bytes());
        let mut target = unsafe { File::create(&c_path[..end + 1])? }; // + 1 for null byte
        let num_bytes = target.write(self.content.as_bytes())?;

        Ok(success(
            &[("bytes_written", num_bytes)],
            &[("path", &self.path), ("message", "Write completed.")],
        ))
    }

    fn display(&self) -> ToolDisplay {
        ToolDisplay {
            name: "Write ",
            arguments: self.path.clone(),
        }
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

impl ActiveTool for EditTool {
    fn run(&self) -> OrtResult<String> {
        let mut content = utils::filename_read_to_string(&self.path)
            .map_err(|str_err| ort_error(ErrorKind::Other, str_err))?;
        let Some(idx) = content.find(&self.old_text) else {
            return Ok("old_text not found in ".to_string() + &self.path);
        };
        if self.replace_all {
            content = content.replace(&self.old_text, &self.new_text);
        } else {
            content.replace_range(idx..idx + self.old_text.len(), &self.new_text);
        }

        let c_path = CString::new(self.path.as_str())
            .map_err(|_err| ort_error(ErrorKind::Other, "Edit path contains nul byte"))?;
        let mut target = unsafe { File::create(c_path.as_bytes_with_nul())? };
        target.write(content.as_bytes())?;

        Ok(success(&[], &[("path", &self.path)]))
    }

    fn display(&self) -> ToolDisplay {
        ToolDisplay {
            name: "Edit ",
            arguments: self.path.clone(),
        }
    }
}

// Helper for tool run Ok return.
fn success(nums: &[(&'static str, usize)], strs: &[(&'static str, &str)]) -> String {
    // String length of a usize is it's number of digits
    let mut len = nums
        .iter()
        .map(|(_, val)| if *val == 0 { 1 } else { val.ilog10() + 1 } as usize)
        .sum();
    len += strs.iter().map(|(_, val)| val.len()).sum::<usize>();

    let mut out = String::with_capacity(len);
    out.push_str(r#"{"success": true"#);

    for (key, num) in nums {
        out.push_str(r#", ""#);
        out.push_str(key);
        out.push_str(r#"": "#);
        let num_s = utils::num_to_string(*num);
        out.push_str(&num_s);
    }

    for (key, s) in strs {
        out.push_str(r#", ""#);
        out.push_str(key);
        out.push_str(r#"": "#);
        // With JSON escaping
        let _ = crate::input::to_json::write_json_str(&mut out, s);
    }

    out.push('}');

    out
}

#[cfg(test)]
mod test {
    use super::success;
    #[test]
    pub fn test_success() {
        let res = success(
            &[("bytes_written", 42)],
            &[
                ("path", "/home/graham/Temp/xyz.txt"),
                ("message", "Write completed."),
            ],
        );
        crate::utils::print_string(c"OUTPUT: ", &res);
    }
}
