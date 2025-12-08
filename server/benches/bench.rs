// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    rc::Rc,
    time::{Duration, Instant},
};

use aircommon::identifiers::UserId;
use airserver_test_harness::utils::setup::TestBackend;
use criterion::{Criterion, criterion_group, criterion_main};
use tokio::sync::Mutex;
use uuid::Uuid;

const NUM_THREADS: usize = 4;

fn benchmarks(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(NUM_THREADS)
        .enable_all()
        .build()
        .unwrap();

    let setup = runtime.block_on(TestBackend::single());
    let setup = Rc::new(Mutex::new(setup));

    let alice = UserId::new(Uuid::new_v4(), "example.com".parse().unwrap());
    let bob = UserId::new(Uuid::new_v4(), "example.com".parse().unwrap());

    let chat_alice_bob = runtime.block_on(async {
        let mut setup = setup.lock().await;
        let alice = setup.add_user().await;
        let bob = setup.add_user().await;
        setup.connect_users(&alice, &bob).await
    });

    let mut group = c.benchmark_group("benchmarks");

    // We reduce the sample size to make `invite_to_group` pass. Otherwise, we run out of key packages.
    group.sample_size(30);

    group.bench_function("add_user", |b| {
        b.to_async(&runtime).iter_custom(|iter| {
            let setup = setup.clone();
            let mut elapsed = Duration::default();
            async move {
                let mut setup = setup.lock().await;
                for _ in 0..iter {
                    let time = Instant::now();
                    setup.add_user().await;
                    elapsed += time.elapsed();
                }
                elapsed
            }
        })
    });

    group.bench_function("connect_users", |b| {
        b.to_async(&runtime).iter_custom(|iter| {
            let alice = alice.clone();
            let setup = setup.clone();
            let mut elapsed = Duration::default();
            async move {
                let mut setup = setup.lock().await;
                for _ in 0..iter {
                    let bob = setup.add_user().await;
                    let time = Instant::now();
                    setup.connect_users(&alice, &bob).await;
                    elapsed += time.elapsed();
                }
                elapsed
            }
        })
    });

    group.bench_function("send_message", |b| {
        b.to_async(&runtime).iter_custom(|iter| {
            let setup = setup.clone();
            let alice = alice.clone();
            let bob = bob.clone();
            let mut elapsed = Duration::default();

            async move {
                let mut setup = setup.lock().await;
                for _ in 0..iter {
                    let time = Instant::now();
                    setup.send_message(chat_alice_bob, &alice, vec![&bob]).await;
                    elapsed += time.elapsed()
                }
                // update group, otherwise we get too far in to the future error from MLS
                setup.update_group(chat_alice_bob, &alice).await;
                elapsed
            }
        })
    });

    const NUM_USERS: usize = 10;
    let mut bobs = Vec::new();
    runtime.block_on(async {
        let mut setup = setup.lock().await;
        for _ in 0..NUM_USERS {
            let bob = setup.add_user().await;
            setup.connect_users(&alice, &bob).await;
            bobs.push(bob);
        }
    });

    group.bench_function("invite_to_group", |b| {
        b.to_async(&runtime).iter_custom(|iter| {
            let setup = setup.clone();
            let alice = alice.clone();
            let bobs = bobs.clone();
            let mut elapsed = Duration::default();
            async move {
                let mut setup = setup.lock().await;
                for _ in 0..iter {
                    // Create an independent group for Alice
                    let chat_id = setup.create_group(&alice).await;
                    let bobs = bobs.iter().collect();
                    let time = Instant::now();
                    setup.invite_to_group(chat_id, &alice, bobs).await;
                    elapsed += time.elapsed();
                }
                elapsed
            }
        });
    });

    let chat_id = runtime.block_on(async {
        let mut setup = setup.lock().await;
        let chat_id = setup.create_group(&alice).await;
        setup
            .invite_to_group(chat_id, &alice, bobs.iter().collect())
            .await;
        chat_id
    });

    group.bench_function("send_message_to_group", |b| {
        b.to_async(&runtime).iter_custom(|iter| {
            let setup = setup.clone();
            let alice = alice.clone();
            let bobs = bobs.clone();
            let mut elapsed = Duration::default();
            async move {
                let mut setup = setup.lock().await;
                for _ in 0..iter {
                    let bobs = bobs.iter().collect();
                    let time = Instant::now();
                    setup.send_message(chat_id, &alice, bobs).await;
                    elapsed += time.elapsed();
                }
                elapsed
            }
        });
    });

    group.finish();
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
