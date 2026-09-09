// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::jstring,
};

use crate::background_execution::processing::{
    init_dismissal_environment, init_environment, init_mark_as_read_environment,
    init_reply_environment,
};
use tracing::error;

/// This method gets called from the Android Messaging Service
#[unsafe(export_name = "Java_ms_air_NativeLib_process_1new_1messages")]
pub extern "C" fn process_new_messages(
    mut env: JNIEnv,
    _class: JClass,
    content: JString,
) -> jstring {
    // Convert Java string to Rust string
    let input: String = match env.get_string(&content) {
        Ok(value) => value.into(),
        Err(_error) => {
            let _ = env.throw_new(
                "java/lang/IllegalArgumentException",
                "Failed to read content string from Java",
            );
            return std::ptr::null_mut();
        }
    };

    let batch = match init_environment(&input) {
        Some(batch) => batch,
        None => {
            let _ = env.throw_new(
                "java/lang/RuntimeException",
                "Failed to process new messages",
            );
            return std::ptr::null_mut();
        }
    };

    let response = match serde_json::to_string(&batch) {
        Ok(json) => json,
        Err(error) => {
            error!(%error, "Failed to serialize notification batch");
            let _ = env.throw_new(
                "java/lang/RuntimeException",
                "Failed to serialize notification batch",
            );
            return std::ptr::null_mut();
        }
    };

    // Convert Rust string back to Java string
    match env.new_string(response) {
        Ok(output) => output.into_raw(),
        Err(error) => {
            error!(%error, "Failed to create Java string from Rust");
            let _ = env.throw_new(
                "java/lang/RuntimeException",
                "Failed to create Java string from Rust",
            );
            std::ptr::null_mut()
        }
    }
}

/// This method gets called from the notification dismissal `BroadcastReceiver` to persist the
/// chat's `notified_until` watermark.
#[unsafe(export_name = "Java_ms_air_NativeLib_notification_1dismissed")]
pub extern "C" fn notification_dismissed(
    mut env: JNIEnv,
    _class: JClass,
    content: JString,
) -> jstring {
    // Convert Java string to Rust string
    let input: String = match env.get_string(&content) {
        Ok(value) => value.into(),
        Err(_error) => {
            let _ = env.throw_new(
                "java/lang/IllegalArgumentException",
                "Failed to read content string from Java",
            );
            return std::ptr::null_mut();
        }
    };

    if init_dismissal_environment(&input).is_none() {
        let _ = env.throw_new(
            "java/lang/RuntimeException",
            "Failed to process notification dismissal",
        );
        return std::ptr::null_mut();
    }

    // Convert Rust string back to Java string
    match env.new_string("") {
        Ok(output) => output.into_raw(),
        Err(error) => {
            error!(%error, "Failed to create Java string from Rust");
            let _ = env.throw_new(
                "java/lang/RuntimeException",
                "Failed to create Java string from Rust",
            );
            std::ptr::null_mut()
        }
    }
}

/// This method gets called from the "Mark as read" notification action `BroadcastReceiver` to
/// mark the chat as read and send the read receipt.
///
/// Returns an empty string on success; throws on failure.
#[unsafe(export_name = "Java_ms_air_NativeLib_mark_1as_1read")]
pub extern "C" fn mark_as_read(mut env: JNIEnv, _class: JClass, content: JString) -> jstring {
    let input: String = match env.get_string(&content) {
        Ok(value) => value.into(),
        Err(_error) => {
            let _ = env.throw_new(
                "java/lang/IllegalArgumentException",
                "Failed to read content string from Java",
            );
            return std::ptr::null_mut();
        }
    };

    if init_mark_as_read_environment(&input).is_none() {
        let _ = env.throw_new(
            "java/lang/RuntimeException",
            "Failed to process mark-as-read",
        );
        return std::ptr::null_mut();
    }

    match env.new_string("") {
        Ok(output) => output.into_raw(),
        Err(error) => {
            error!(%error, "Failed to create Java string from Rust");
            let _ = env.throw_new(
                "java/lang/RuntimeException",
                "Failed to create Java string from Rust",
            );
            std::ptr::null_mut()
        }
    }
}

/// This method gets called from the "Reply" notification action `BroadcastReceiver` to compose
/// and send the typed message.
///
/// Returns an empty string on success; throws on failure.
#[unsafe(export_name = "Java_ms_air_NativeLib_reply")]
pub extern "C" fn reply(mut env: JNIEnv, _class: JClass, content: JString) -> jstring {
    let input: String = match env.get_string(&content) {
        Ok(value) => value.into(),
        Err(_error) => {
            let _ = env.throw_new(
                "java/lang/IllegalArgumentException",
                "Failed to read content string from Java",
            );
            return std::ptr::null_mut();
        }
    };

    if init_reply_environment(&input).is_none() {
        let _ = env.throw_new("java/lang/RuntimeException", "Failed to process reply");
        return std::ptr::null_mut();
    }

    match env.new_string("") {
        Ok(output) => output.into_raw(),
        Err(error) => {
            error!(%error, "Failed to create Java string from Rust");
            let _ = env.throw_new(
                "java/lang/RuntimeException",
                "Failed to create Java string from Rust",
            );
            std::ptr::null_mut()
        }
    }
}
