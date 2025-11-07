// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::{CStr, CString, c_char};

use crate::background_execution::processing::init_environment;

/// This method gets called from the iOS NSE
///
/// # Safety
///
/// The caller must ensure that the content is a pointer to a valid C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn process_new_messages(content: *const c_char) -> *mut c_char {
    if content.is_null() {
        return std::ptr::null_mut();
    }

    // Borrow the incoming C string (must be NUL-terminated)
    let c_str = unsafe { CStr::from_ptr(content) };

    // Ensure it's valid UTF-8 (JSON must be UTF-8)
    let json_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => {
            return std::ptr::null_mut();
        }
    };

    // Initialize the environment and retrieve the notification batch
    let batch = match init_environment(json_str) {
        Some(batch) => batch,
        None => return std::ptr::null_mut(),
    };

    // Serialize the response JSON and return an owned C string to the caller
    match serde_json::to_string(&batch)
        .ok()
        .and_then(|s| CString::new(s).ok())
    {
        Some(cstr) => cstr.into_raw(),
        None => std::ptr::null_mut(),
    }
}

/// This method gets called from the iOS NSE
///
/// # Safety
///
/// The caller must ensure that the input string was previously created by
/// `process_new_messages`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(s);
    }
}
