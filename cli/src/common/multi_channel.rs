//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::sync::mpsc::{Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::OrtResult;

// Returning OrtResult makes the Vec<JoinHandle> work, and gives options for
// error handling in the future.
pub fn broadcast<T: Clone + Send + 'static>(
    rx: Receiver<T>,
    senders: Vec<Sender<T>>,
) -> JoinHandle<OrtResult<()>> {
    thread::spawn(move || -> OrtResult<()> {
        while let Ok(msg) = rx.recv() {
            for sender in &senders {
                // Ignore send errors, as a sender might have been dropped.
                let _ = sender.send(msg.clone());
            }
        }
        Ok(())
    })
}
