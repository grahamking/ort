//! art: Open Router Agent
//! Part of the `ort` project
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use std::fs;
use std::path::Path;

use crate::ort_error;
use ort_openrouter_cli::{Content, ErrorKind, Message, OrtResult, Role};

pub const DEFAULT: &str = r#"You are an expert coding assistant operating inside ort, a coding agent harness running in a Linux terminal. You help users by reading files, executing commands, editing code, and writing new files.

Available tools:
- read: Read file contents
- bash: Execute bash commands
- edit: Make surgical edits to files
- write: Create or overwrite files

Guidelines:
- Use bash for file operations like ls, rg, find
- Use bash with rg (ripgrep) to search for text in files
- Use read to examine files instead of cat or sed
- Use read to examine files before editing
- Use write only for new files or complete rewrites
- Use edit for precise changes
- Be concise in your responses
- Show file paths clearly when working with files

When to make a plan:
- For non-trivial tasks that involve using tools or editing files, first produce a short plan with 3 to 7 steps.
- Update the plan when new information changes the approach.
- Do not plan simple read-only tasks.

Current date and time: $DATE
Current working directory: $PWD
"#;

const SKILLS_INTRO: &str = r#"## Skills
The following skills provide specialized instructions for specific tasks.
Use the `read` tool to load a skill's `SKILL.md` file when the task matches its description.
When a skill file references a relative path, resolve it against the skill directory (parent of `SKILL.md`).

### Available skills"#;

/// Add message to system prompt
pub(crate) fn add(messages: &mut [Message], extra_content: &str) -> OrtResult<()> {
    match messages.first_mut() {
        Some(Message {
            role: Role::System,
            content: content_vec,
            ..
        }) => match content_vec.first_mut() {
            Some(Content::Text(s)) => {
                s.push('\n');
                s.push_str(extra_content);
                Ok(())
            }
            _ => Err(ort_error(ErrorKind::MissingSystemPrompt, "No content")),
        },
        _ => Err(ort_error(
            ErrorKind::MissingSystemPrompt,
            "No system prompt",
        )),
    }
}

pub(crate) fn load_skills(skills_dir: &Path) -> String {
    let Ok(entries) = fs::read_dir(skills_dir) else {
        return String::new();
    };
    let mut dirs: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut skill_lines = Vec::new();
    let mut skill_size = 0;
    for dir in dirs {
        let skill_md = dir.join("SKILL.md");
        let Ok(contents) = fs::read_to_string(&skill_md) else {
            continue;
        };
        let fallback_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if let Some((name, description)) = parse_skill_preamble(&contents, fallback_name) {
            let path_str = skill_md.to_string_lossy().replace('\\', "/");
            let s = format!("- {name}: {description} (file: {path_str})");
            skill_size += s.len() + 1; // +1 for the \n we add later
            skill_lines.push(s);
        }
    }

    if skill_lines.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(SKILLS_INTRO.len() + 1 + skill_size);
    out.push_str(SKILLS_INTRO);
    out.push('\n');
    out.push_str(&skill_lines.join("\n"));
    out
}

// Skills are YAML, so we have to implement a basic YAML parser
fn parse_skill_preamble(contents: &str, fallback_name: &str) -> Option<(String, String)> {
    let mut lines = contents.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut frontmatter = Vec::new();
    let mut found_closing = false;
    for line in lines {
        if line.trim() == "---" {
            found_closing = true;
            break;
        }
        frontmatter.push(line);
    }
    if !found_closing {
        return None;
    }

    let mut name = None;
    let mut description = None;
    let mut idx = 0;
    while idx < frontmatter.len() {
        let line = frontmatter[idx];
        let trimmed = line.trim();
        idx += 1;
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Only match top-level keys (not indented under metadata:, etc.)
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let Some((key, val)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        match key {
            "name" => {
                let parsed = unquote_yaml_scalar(val);
                if !parsed.is_empty() {
                    name = Some(parsed);
                }
            }
            "description" => {
                if matches!(val, ">" | "|" | ">-" | "|-" | ">+" | "|+" | "") {
                    let mut multiline = Vec::new();
                    while idx < frontmatter.len() {
                        let next_line = frontmatter[idx];
                        if next_line.starts_with(' ') || next_line.starts_with('\t') {
                            let part = next_line.trim();
                            if !part.is_empty() {
                                multiline.push(part);
                            }
                            idx += 1;
                        } else if next_line.trim().is_empty() {
                            idx += 1;
                        } else {
                            break;
                        }
                    }
                    let joined = multiline.join(" ");
                    if !joined.is_empty() {
                        description = Some(joined);
                    }
                } else {
                    let parsed = unquote_yaml_scalar(val);
                    if !parsed.is_empty() {
                        description = Some(parsed);
                    }
                }
            }
            _ => {}
        }
    }

    let name = name.unwrap_or_else(|| fallback_name.trim().to_string());
    let description = description?;
    if name.is_empty() || description.is_empty() {
        return None;
    }
    Some((name, description))
}

fn unquote_yaml_scalar(val: &str) -> String {
    let val = val.trim();
    if val.len() >= 2
        && ((val.starts_with('"') && val.ends_with('"'))
            || (val.starts_with('\'') && val.ends_with('\'')))
    {
        val[1..val.len() - 1].trim().to_string()
    } else {
        val.to_string()
    }
}
