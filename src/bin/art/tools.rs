//! art: Open Router Agent
//! Part of the `ort` project
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::boxed::Box;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use std::fs;
use std::io::{BufRead as _, BufReader};

use ort_openrouter_cli::{
    ErrorKind, Function, OrtResult, Tool, ToolDisplay, ToolParameter, Write, file, json_parser,
    num_to_string, ort_err, syscall::system, write_json_str,
};

pub const ALL_TOOLS: &[&Tool] = &[&TOOL_READ, &TOOL_BASH, &TOOL_WRITE, &TOOL_EDIT];

/// How many lines to read in the Read tool if model does not specify
pub const DEFAULT_READ_LIMIT: usize = 2000;

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
            description: "Line number to start reading from (0-indexed)",
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
    parameters: &[
        ToolParameter {
            name: "command",
            param_type: "string",
            description: "Bash command to execute",
        },
        ToolParameter {
            name: "limit",
            param_type: "number",
            description: "Maximum number of lines to return",
        },
    ],
    required_parameters: &["command"],
};

const TOOL_WRITE: Tool = Tool {
    name: "write",
    description: "Write content to a file. Creates the file if it doesn't exist. Refuses to overwrite an existing file unless overwrite is true. Automatically creates parent directories. Use only for new files or complete rewrites.",
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
        ToolParameter {
            name: "overwrite",
            param_type: "boolean",
            description: "If true, allow overwriting an existing file. Defaults to false.",
        },
    ],
    required_parameters: &["path", "content"],
};

const TOOL_EDIT: Tool = Tool {
    name: "edit",
    description: "Edit a file by replacing an exact old_text span with new_text. By default old_text must occur exactly once. If expected_occurrences is provided, old_text must occur exactly that many times and all occurrences are replaced.",
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
            name: "expected_occurrences",
            param_type: "number",
            description: "If provided, fail unless old_text occurs exactly this many times before editing.",
        },
    ],
    required_parameters: &["path", "old_text", "new_text"],
};

pub fn parse_function(f: &Function) -> OrtResult<Box<dyn ActiveTool>> {
    match f.name.as_ref() {
        "read" => {
            let t = ReadTool::from_json(&f.arguments).map_err(|err| {
                ort_err(
                    ErrorKind::ParsingToolCallParams,
                    ("Parsing read tool params JSON - ".to_string() + &err.as_string()).into(),
                )
            })?;
            Ok(Box::new(t))
        }
        "bash" => {
            let t = BashTool::from_json(&f.arguments).map_err(|err| {
                ort_err(
                    ErrorKind::ParsingToolCallParams,
                    ("Parsing bash tool params JSON '".to_string()
                        + &f.arguments
                        + "' - "
                        + &err.as_string())
                        .into(),
                )
            })?;
            Ok(Box::new(t))
        }
        "write" => {
            let t = WriteTool::from_json(&f.arguments).map_err(|err| {
                ort_err(
                    ErrorKind::ParsingToolCallParams,
                    ("Parsing write tool params JSON - ".to_string() + &err.as_string()).into(),
                )
            })?;
            Ok(Box::new(t))
        }
        "edit" => {
            let t = EditTool::from_json(&f.arguments).map_err(|err| {
                ort_err(
                    ErrorKind::ParsingToolCallParams,
                    ("Parsing edit tool params JSON - ".to_string() + &err.as_string()).into(),
                )
            })?;
            Ok(Box::new(t))
        }
        missing => Err(ort_err(
            ErrorKind::ToolDoesNotExist,
            missing.to_string().into(),
        )),
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
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            json_parser::JsonField::new_simple_string("path"),
            json_parser::JsonField::new_int("offset"),
            json_parser::JsonField::new_int("limit"),
        ];
        json_parser::autoparser(json, &mut fields)?;
        Ok(ReadTool {
            path: fields[0].get_string().expect("Missing ReadTool path"),
            offset: fields[1].get_int(),
            limit: fields[2].get_int(),
        })
    }
}

impl ActiveTool for ReadTool {
    fn run(&self) -> OrtResult<String> {
        let f = match fs::File::open(&self.path) {
            Ok(f) => f,
            // Return the string error so the model sees it.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(ort_err(
                    ErrorKind::ToolRun,
                    ("No such file or directory: ".to_string() + &self.path).into(),
                ));
            }
            Err(err) => {
                return Err(ort_err(
                    ErrorKind::ToolRun,
                    ("Tool call error ".to_string() + &err.to_string() + ": " + &self.path).into(),
                ));
            }
        };
        let metadata = f
            .metadata()
            .map_err(|err| ort_err(ErrorKind::ToolRun, err.to_string().into()))?;

        let offset = self.offset.map_or(0, |offset| offset as usize);
        let limit = self
            .limit
            .map_or(DEFAULT_READ_LIMIT, |limit| limit as usize);

        let reader = BufReader::new(f);
        let content_lines: Vec<String> = reader
            .lines()
            .skip(offset)
            // We read one past the end to check if there is more
            .take(limit + 1)
            .filter_map(|l| l.ok())
            .collect();
        let num_lines = content_lines.len();
        let is_truncated = if num_lines > limit { "true" } else { "false" };
        let content = if is_truncated == "true" {
            content_lines[..limit].join("\n")
        } else {
            content_lines.join("\n")
        };

        Ok(success(
            &[
                ("lines", num_lines),
                ("file_size_in_bytes", metadata.len() as usize),
            ],
            &[
                ("path", &self.path),
                ("is_truncated", is_truncated),
                ("output", &content),
            ],
        ))
    }

    fn display(&self) -> ToolDisplay {
        let offset = self.offset.unwrap_or(0);
        let limit = self.limit.unwrap_or(DEFAULT_READ_LIMIT as u32);
        let extra = " lines ".to_string()
            + &num_to_string(offset)
            + "-"
            + &num_to_string(offset.saturating_add(limit));
        ToolDisplay {
            name: "Read ",
            arguments: self.path.clone(),
            extra: Some(extra),
        }
    }
}

pub struct BashTool {
    pub command: String,
    pub limit: Option<u32>,
}

impl BashTool {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            json_parser::JsonField::new_string("command"),
            json_parser::JsonField::new_int("limit"),
        ];
        json_parser::autoparser(json, &mut fields)?;
        Ok(BashTool {
            command: fields[0].get_string().expect("Missing BashTool command"),
            limit: fields[1].get_int(),
        })
    }
}

impl ActiveTool for BashTool {
    fn run(&self) -> OrtResult<String> {
        let output = system(&self.command)?;
        let stdout = limit_lines(&output.stdout, self.limit);
        let stderr = limit_lines(&output.stderr, self.limit);
        Ok(success(
            &[("exit_code", output.exit_code as usize)],
            &[("stdout", &stdout), ("stderr", &stderr)],
        ))
    }

    fn display(&self) -> ToolDisplay {
        ToolDisplay {
            name: "Bash ",
            arguments: self.command.clone(),
            extra: self
                .limit
                .map(|limit| " limit ".to_string() + &num_to_string(limit)),
        }
    }
}

pub struct WriteTool {
    pub path: String,
    pub content: String,
    pub overwrite: bool,
}

impl WriteTool {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            json_parser::JsonField::new_simple_string("path"),
            json_parser::JsonField::new_string("content"),
            json_parser::JsonField::new_bool("overwrite"),
        ];
        json_parser::autoparser(json, &mut fields)?;
        Ok(WriteTool {
            path: fields[0].get_string().expect("Missing WriteTool path"),
            content: fields[1].get_string().expect("Missing WriteTool content"),
            overwrite: fields[2].get_bool().unwrap_or(false),
        })
    }
}

impl ActiveTool for WriteTool {
    fn run(&self) -> OrtResult<String> {
        match fs::metadata(&self.path) {
            Ok(_) if !self.overwrite => {
                let msg = "write refuses to overwrite existing file without overwrite=true: "
                    .to_string()
                    + &self.path;
                return Err(ort_err(ErrorKind::ToolRun, msg.into()));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                let msg = "write failed to check existing file ".to_string()
                    + &self.path
                    + " - "
                    + &err.to_string();
                return Err(ort_err(ErrorKind::ToolRun, msg.into()));
            }
        }

        if let Some(idx) = self.path.rfind('/') {
            let dir_path = &self.path[..idx];
            let _ = fs::create_dir_all(dir_path);
        }

        // Write the file
        let mut c_path = [0u8; 128];
        let end = self.path.len();
        c_path[..end].copy_from_slice(self.path.as_bytes());
        let mut target = unsafe { file::File::create(&c_path[..end + 1])? }; // + 1 for null byte
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
            extra: None,
        }
    }
}

pub struct EditTool {
    pub path: String,
    pub old_text: String,
    pub new_text: String,
    pub expected_occurrences: Option<u32>,
}

impl EditTool {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        // Example JSON:
        // { "path": "LICENSE",
        //   "old_text": "Copyright (c) 2025 Graham King",
        //   "new_text": "Copyright (c) 2025, 2026 Graham King"
        // }
        let mut fields = [
            json_parser::JsonField::new_simple_string("path"),
            json_parser::JsonField::new_string("old_text"),
            json_parser::JsonField::new_string("new_text"),
            json_parser::JsonField::new_int("expected_occurrences"),
        ];
        json_parser::autoparser(json, &mut fields)?;
        Ok(EditTool {
            path: fields[0].get_string().expect("Missing EditTool path"),
            old_text: fields[1].get_string().expect("Missing EditTool old_text"),
            new_text: fields[2].get_string().expect("Missing EditTool new_text"),
            expected_occurrences: fields[3].get_int(),
        })
    }
}

impl ActiveTool for EditTool {
    fn run(&self) -> OrtResult<String> {
        if self.old_text.is_empty() {
            return Err(ort_err(
                ErrorKind::ToolRun,
                "edit old_text cannot be empty".into(),
            ));
        }

        let mut content = fs::read_to_string(&self.path).map_err(|err| {
            let msg =
                "filename_read_to_string ".to_string() + &self.path + " - " + &err.to_string();
            ort_err(ErrorKind::ToolRun, msg.into())
        })?;
        let occurrences = content.matches(&self.old_text).count();
        self.validate_occurrences(occurrences)?;

        if self.expected_occurrences.is_some() {
            content = content.replace(&self.old_text, &self.new_text);
        } else {
            let Some(idx) = content.find(&self.old_text) else {
                return Err(ort_err(
                    ErrorKind::ToolRun,
                    ("old_text not found in ".to_string() + &self.path).into(),
                ));
            };
            content.replace_range(idx..idx + self.old_text.len(), &self.new_text);
        }

        let c_path = CString::new(self.path.as_str()).map_err(|_err| {
            let msg = self.path.to_string() + ". Edit path contains nul byte";
            ort_err(ErrorKind::ToolRun, msg.into())
        })?;
        let mut target = unsafe { file::File::create(c_path.as_bytes_with_nul())? };
        target.write(content.as_bytes())?;

        Ok(success(&[], &[("path", &self.path)]))
    }

    fn display(&self) -> ToolDisplay {
        ToolDisplay {
            name: "Edit ",
            arguments: self.path.clone(),
            extra: Some(" lines ".to_string() + &num_to_string(self.old_text.lines().count())),
        }
    }
}

impl EditTool {
    fn validate_occurrences(&self, actual: usize) -> OrtResult<()> {
        if let Some(expected) = self.expected_occurrences {
            if expected == 0 {
                return Err(ort_err(
                    ErrorKind::ToolRun,
                    "edit expected_occurrences must be greater than zero".into(),
                ));
            }
            if actual != expected as usize {
                let msg = "edit expected_occurrences mismatch in ".to_string()
                    + &self.path
                    + ": expected "
                    + &expected.to_string()
                    + ", found "
                    + &actual.to_string();
                return Err(ort_err(ErrorKind::ToolRun, msg.into()));
            }
            return Ok(());
        }

        if actual == 0 {
            return Err(ort_err(
                ErrorKind::ToolRun,
                ("old_text not found in ".to_string() + &self.path).into(),
            ));
        }

        if actual != 1 {
            let msg = "edit old_text is ambiguous in ".to_string()
                + &self.path
                + ": found "
                + &actual.to_string()
                + " occurrences";
            return Err(ort_err(ErrorKind::ToolRun, msg.into()));
        }

        Ok(())
    }
}

fn limit_lines(content: &str, limit: Option<u32>) -> String {
    match limit {
        Some(limit) => content
            .lines()
            .take(limit as usize)
            .collect::<Vec<_>>()
            .join("\n"),
        None => content.to_string(),
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
        out.push_str(&num.to_string());
    }

    for (key, s) in strs {
        out.push_str(r#", ""#);
        out.push_str(key);
        out.push_str(r#"": "#);
        // With JSON escaping
        let _ = write_json_str(&mut out, s);
    }

    out.push('}');

    out
}

#[cfg(test)]
mod test {
    use super::{ActiveTool, EditTool, success};

    fn temp_path(name: &str) -> String {
        let mut path = std::env::temp_dir();
        path.push(format!("ort_art_tools_{name}_{}", std::process::id()));
        path.to_string_lossy().into_owned()
    }

    #[test]
    pub fn test_success() {
        let res = success(
            &[("bytes_written", 42)],
            &[
                ("path", "/home/graham/Temp/xyz.txt"),
                ("message", "Write completed."),
            ],
        );
        let expected = r#"{"success": true, "bytes_written": 42, "path": "/home/graham/Temp/xyz.txt", "message": "Write completed."}"#;
        assert_eq!(res, expected);
    }

    #[test]
    pub fn edit_validates_occurrence_count_before_replacing() {
        let path = temp_path("edit_occurrences");

        std::fs::write(&path, "alpha\nalpha\n").unwrap();
        let ambiguous = EditTool {
            path: path.clone(),
            old_text: "alpha".to_string(),
            new_text: "beta".to_string(),
            expected_occurrences: None,
        }
        .run();
        assert!(ambiguous.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "alpha\nalpha\n");

        EditTool {
            path: path.clone(),
            old_text: "alpha".to_string(),
            new_text: "beta".to_string(),
            expected_occurrences: Some(2),
        }
        .run()
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "beta\nbeta\n");

        std::fs::write(&path, "alpha\nalpha\n").unwrap();
        let mismatch = EditTool {
            path: path.clone(),
            old_text: "alpha".to_string(),
            new_text: "beta".to_string(),
            expected_occurrences: Some(1),
        }
        .run();
        assert!(mismatch.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "alpha\nalpha\n");

        let _ = std::fs::remove_file(path);
    }
}
