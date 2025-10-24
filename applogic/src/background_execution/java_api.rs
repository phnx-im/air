// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::jstring,
};

use crate::background_execution::processing::init_environment;
use tracing::error;

/// This methos gets called from the Android Messaging Service
#[unsafe(export_name = "Java_ms_air_NativeLib_process_1new_1messages")]
pub extern "C" fn process_new_messages(
    mut env: JNIEnv,
    _class: JClass,
    content: JString,
) -> jstring {
    // Convert Java string to Rust string
    let input: String = match env.get_string(&content) {
        Ok(value) => value.into(),
        Err(error) => {
            error!(%error, "Failed to read content string from Java");
            return std::ptr::null_mut();
        }
    };

    let batch = match init_environment(&input) {
        Some(batch) => batch,
        None => return std::ptr::null_mut(),
    };

    let response = match serde_json::to_string(&batch) {
        Ok(json) => json,
        Err(error) => {
            error!(%error, "Failed to serialize notification batch");
            return std::ptr::null_mut();
        }
    };

    // Convert Rust string back to Java string
    match env.new_string(response) {
        Ok(output) => output.into_raw(),
        Err(error) => {
            error!(%error, "Failed to create Java string");
            std::ptr::null_mut()
        }
    }
}
