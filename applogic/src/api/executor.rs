// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Process-wide thread pools behind the FRB handler.
//!
//! The FRB defaults size the tokio runtime and the sync thread pool at one
//! thread per CPU each, and rayon does the same for its global pool. A UI
//! client needs a handful of threads, so all three are capped here.

use std::{panic::AssertUnwindSafe, sync::LazyLock};

use flutter_rust_bridge::{
    DefaultHandler, SimpleAsyncRuntime, SimpleThreadPool,
    for_generated::{FLUTTER_RUST_BRIDGE_RUNTIME_VERSION, NoOpErrorListener, SimpleExecutor},
};

use crate::FLUTTER_RUST_BRIDGE_CODEGEN_VERSION;

const TOKIO_WORKER_THREADS: usize = 6;
const SYNC_POOL_THREADS: usize = 2;
const RAYON_THREADS: usize = 2;

/// Replaces FRB's default handler. FRB codegen detects this static and
/// re-exports it from the generated code.
pub static FLUTTER_RUST_BRIDGE_HANDLER: LazyLock<DefaultHandler<SimpleThreadPool>> =
    LazyLock::new(build_handler);

fn build_handler() -> DefaultHandler<SimpleThreadPool> {
    assert_eq!(
        FLUTTER_RUST_BRIDGE_CODEGEN_VERSION, FLUTTER_RUST_BRIDGE_RUNTIME_VERSION,
        "flutter_rust_bridge codegen ({FLUTTER_RUST_BRIDGE_CODEGEN_VERSION}) and runtime \
         ({FLUTTER_RUST_BRIDGE_RUNTIME_VERSION}) versions differ"
    );

    init_rayon();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(TOKIO_WORKER_THREADS)
        .thread_name("tokio-worker")
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    let thread_pool = threadpool::Builder::new()
        .num_threads(SYNC_POOL_THREADS)
        .thread_name("frb-sync".into())
        .build();

    DefaultHandler::new(
        SimpleExecutor::new(
            NoOpErrorListener,
            SimpleThreadPool(thread_pool),
            SimpleAsyncRuntime(AssertUnwindSafe(runtime)),
        ),
        NoOpErrorListener,
    )
}

/// Caps the rayon global pool used by openmls. Fails only if the pool is
/// already in use, in which case it keeps its default size.
fn init_rayon() {
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(RAYON_THREADS)
        .thread_name(|i| format!("rayon-{i}"))
        .build_global();
}
