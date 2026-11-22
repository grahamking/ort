//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{Message, Role};

#[derive(Clone)]
pub struct Conversation {
    m: Vec<Message>,
    prepended: usize,
}

impl Conversation {
    pub fn new() -> Self {
        // A Message is quite small, an enum and two Option<String>.
        // Capacity 3 for (all optional):
        // - System message
        // - User message
        // - and the assistant message that LastWriter appends, to save a realloc.
        Self::from_messages(Vec::with_capacity(3))
    }

    pub fn from_messages(m: Vec<Message>) -> Self {
        Conversation { m, prepended: 0 }
    }

    pub fn set_instructions(&mut self, s: String) {
        if self.has_system_message() {
            self.m[0] = Message::system(s);
        } else {
            self.m.insert(0, Message::system(s));
        }
    }

    fn has_system_message(&self) -> bool {
        self.m
            .first()
            .is_some_and(|m| matches!(m.role, Role::System))
    }

    /// Put a message first in the conversation, but after the system message, if present.
    pub fn prepend_message(&mut self, s: String) {
        self.m
            .insert(usize::from(self.has_system_message()), Message::user(s));
        self.prepended += 1;
    }

    /// Number of initial messages to keep verbatim when replacing the history.
    pub fn retained_prefix_len(&self) -> usize {
        usize::from(self.has_system_message()) + self.prepended
    }

    /// Replace history while leaving the retained prefix intact.
    pub fn replace_history(&mut self, history: impl IntoIterator<Item = Message>) {
        self.m.truncate(self.retained_prefix_len());
        self.m.extend(history);
    }

    pub fn add_user_prompt(&mut self, s: impl Into<String>) {
        self.m.push(Message::user(s.into()));
    }

    /// Put a message next in the conversation
    pub fn add_message(&mut self, m: Message) {
        self.m.push(m);
    }

    /// Temporary while we migrate
    pub fn messages(&self) -> &[Message] {
        self.m.as_slice()
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}
