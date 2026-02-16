//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use crate::libc;
use core::alloc::Layout;
use core::ffi::c_void;

#[cfg(feature = "print-allocations")]
use crate::common::utils::to_ascii;

pub struct LibcAlloc;

// In case you were wondering, yes all three methods get used. Rust does
// a bnuch of alloc_zeroed and realloc.
//
// Build with feature "print-allocations" to see memory being allocated:
// cargo build --features="print-allocations"
// cargo build --release --features="print-allocations" -Zbuild-std="core,alloc"
//
// There's a Python script at the end of this file to summarize the output.
//
// Normal / prompt usage seems to peak under 64 Kib of active memory.
//
// `ort list` peaks around 180 Kib because it has one large (~128 KiB)
// allocation which is a string holding the names of all models, so we can sort them.
unsafe impl core::alloc::GlobalAlloc for LibcAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "print-allocations")]
        {
            let mut buf = [0u8; 16];
            buf[0] = b'+';
            let len = to_ascii(layout.size(), &mut buf[1..]);
            unsafe { crate::libc::write(2, buf.as_ptr().cast(), len) };
        }
        unsafe { libc::malloc(layout.size().max(layout.align())) as *mut u8 }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "print-allocations")]
        {
            let mut buf = [0u8; 16];
            buf[0] = b'+';
            let len = to_ascii(layout.size(), &mut buf[1..]);
            unsafe { crate::libc::write(2, buf.as_ptr().cast(), len) };
        }
        unsafe { libc::calloc(1, layout.size().max(layout.align())) as *mut u8 }
    }

    #[allow(unused_variables)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        #[cfg(feature = "print-allocations")]
        {
            let mut buf = [0u8; 16];
            buf[0] = b'-';
            let len = to_ascii(layout.size(), &mut buf[1..]);
            unsafe { crate::libc::write(2, buf.as_ptr().cast(), len) };
        }
        unsafe { libc::free(ptr as *mut c_void) }
    }

    #[allow(unused_variables)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        #[cfg(feature = "print-allocations")]
        {
            let mut buf = [0u8; 16];
            buf[0] = b'\\';
            let len = to_ascii(layout.size(), &mut buf[1..]);
            unsafe { crate::libc::write(2, buf.as_ptr().cast(), len) };

            buf[0] = b'/';
            let len = to_ascii(new_size, &mut buf[1..]);
            unsafe { crate::libc::write(2, buf.as_ptr().cast(), len) };
        }
        unsafe { libc::realloc(ptr as *mut c_void, new_size) as *mut u8 }
    }
}

/*
"""Print running totals from allocs.txt and report the maximum cumulative value."""

from pathlib import Path


def main() -> None:
    path = Path("allocs.txt")
    if not path.is_file():
        raise SystemExit("allocs.txt not found in the current directory")

    total = 0
    max_total = None  # Highest cumulative total
    max_plus = None   # Largest individual + value
    max_minus = None  # Largest (most negative) individual - value

    with path.open() as fh:
        for raw_line in fh:
            line = raw_line.strip()
            if not line:
                continue  # Skip blank lines silently

            try:
                delta = int(line)
            except ValueError as exc:
                raise SystemExit(f"Invalid line in allocs.txt: {line!r}") from exc
            # realloc indicators
            line = line.replace('/', '+').replace('\\', '-')

            if delta > 0:
                max_plus = delta if max_plus is None else max(max_plus, delta)
            elif delta < 0:
                max_minus = delta if max_minus is None else min(max_minus, delta)

            total += delta
            max_total = total if max_total is None else max(max_total, total)
            print(f"{line} {total}")

    print()  # Blank line before the summary, matching the example
    print(f"Max: {max_total if max_total is not None else 0}")
    print(f"Largest +: {max_plus if max_plus is not None else 0}")
    print(f"Largest -: {max_minus if max_minus is not None else 0}")


if __name__ == "__main__":
    main()
*/
