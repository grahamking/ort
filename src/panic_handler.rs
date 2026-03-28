//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King
//!
//! Because release mode uses panic="abort".

use ort_openrouter_cli::syscall;

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    syscall::write(1, "panic: ".as_ptr().cast(), "panic: ".len());
    if let Some(msg) = info.message().as_str() {
        syscall::write(2, msg.as_ptr().cast(), msg.len());
        syscall::write(2, ", ".as_ptr().cast(), 2);
    }
    if let Some(location) = info.location() {
        let f = location.file();
        syscall::write(2, f.as_ptr().cast(), f.len());
        syscall::write(2, ", ".as_ptr().cast(), 2);

        let line = location.line();
        let line_s = ort_openrouter_cli::utils::num_to_string(line);
        syscall::write(2, line_s.as_ptr().cast(), line_s.len());
    }
    syscall::write(1, "\n".as_ptr().cast(), 1);
    syscall::exit(99)
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}
