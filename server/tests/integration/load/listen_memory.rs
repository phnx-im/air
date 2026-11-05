// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server heap memory per open QS Listen connection.

use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use aircommon::{OpenMlsRand, RustCrypto};
use aircoreclient::{
    ChatId, MarkChatAsRead,
    clients::{CoreUser, listen_response},
};
use airserver_test_harness::utils::setup::TestBackend;
use futures_util::{StreamExt, stream};
use mimi_content::MimiContent;
use rand::{RngExt, distr::Alphanumeric};
use tokio::{
    task::JoinHandle,
    time::{MissedTickBehavior, interval, sleep},
};

const SAMPLES: usize = 3;
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// Sums all series of the metric `name` in a Prometheus text scrape.
fn sum_metric(text: &str, name: &str) -> f64 {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let rest = line.strip_prefix(name)?;
            // Skip metrics that only share the name prefix
            if !rest.starts_with([' ', '{']) {
                return None;
            }
            line.rsplit(' ').next()?.parse::<f64>().ok()
        })
        .sum()
}

async fn scrape(url: &str) -> (f64, f64) {
    let text = reqwest::get(url).await.unwrap().text().await.unwrap();
    (
        sum_metric(&text, "air_server_memory_used_bytes"),
        sum_metric(&text, "air_qs_active_users"),
    )
}

/// Enqueues one random message, returns whether it succeeded.
async fn send_one(from: &CoreUser, chat_id: ChatId, size: usize) -> bool {
    let text: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(size)
        .map(char::from)
        .collect();
    let salt: [u8; 16] = RustCrypto::default().random_array().unwrap();
    let content = MimiContent::simple_markdown_message(text, salt);
    match from
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
    {
        Ok(_) => true,
        Err(error) => {
            eprintln!("send failed: {error:#}");
            false
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs TEST_SERVER_URL pointing at a running server"]
async fn listen_memory_per_connection() {
    if env::var("TEST_SERVER_URL").is_err() {
        eprintln!("TEST_SERVER_URL is not set, skipping");
        return;
    }
    let mut steps: Vec<usize> = env::var("LOAD_STEPS")
        .unwrap_or_else(|_| "0,200,400".into())
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    steps.sort_unstable();
    let metrics_url =
        env::var("METRICS_URL").unwrap_or_else(|_| "http://127.0.0.1:9090/metrics".into());
    let settle = Duration::from_secs(
        env::var("LOAD_SETTLE_SECS")
            .map(|s| s.parse().unwrap())
            .unwrap_or(3),
    );
    let rate: f64 = env::var("LOAD_MSG_RATE")
        .map(|s| s.parse().unwrap())
        .unwrap_or(1.0);
    let backlog: usize = env::var("LOAD_BACKLOG")
        .map(|s| s.parse().unwrap())
        .unwrap_or(0);
    let slow_every: usize = env::var("LOAD_SLOW_EVERY")
        .map(|s| s.parse().unwrap())
        .unwrap_or(0);
    let concurrency: usize = env::var("LOAD_CONCURRENCY")
        .map(|s| s.parse().unwrap())
        .unwrap_or(32);
    let msg_size: usize = env::var("LOAD_MSG_SIZE")
        .map(|s| s.parse().unwrap())
        .unwrap_or(32);
    let big_once: bool = env::var("LOAD_BIG_ONCE")
        .map(|s| s.parse().unwrap())
        .unwrap_or(false);
    // Slow listeners are the users with index i % slow_every == 0
    let is_slow = move |i: usize| slow_every > 0 && i.is_multiple_of(slow_every);
    // Round up to whole pairs
    let max = steps.last().unwrap().next_multiple_of(2);

    let mut backend = TestBackend::single().await;
    let start = Instant::now();
    let users = backend.add_users_concurrent(max, concurrency).await;
    println!("registered {max} users in {:.1?}", start.elapsed());

    // Connect users in pairs before any listen is opened
    let start = Instant::now();
    let mut pairs: Vec<(CoreUser, CoreUser, ChatId)> = Vec::with_capacity(max / 2);
    for pair in users.chunks(2) {
        let chat_id = backend.connect_users(&pair[0], &pair[1]).await;
        pairs.push((pair[0].clone(), pair[1].clone(), chat_id));
    }
    println!("connected {} pairs in {:.1?}", pairs.len(), start.elapsed());

    // Fill the queues before any listener connects, one run drains the outbound queue.
    // With slow listeners only their queues get a backlog.
    if backlog > 0 {
        let start = Instant::now();
        stream::iter(
            pairs
                .iter()
                .enumerate()
                .flat_map(|(k, (a, b, chat_id))| {
                    [
                        (a.clone(), *chat_id, 2 * k + 1),
                        (b.clone(), *chat_id, 2 * k),
                    ]
                })
                .filter(|(_, _, to)| slow_every == 0 || is_slow(*to))
                .map(|(from, chat_id, _)| (from, chat_id)),
        )
        .map(|(from, chat_id)| {
            tokio::spawn(async move {
                for _ in 0..backlog {
                    send_one(&from, chat_id, msg_size).await;
                }
                from.outbound_service().run_once().await;
            })
        })
        .buffer_unordered(concurrency)
        .for_each(|joined| async { joined.unwrap() })
        .await;
        println!("backlog sent in {:.1?}", start.elapsed());
    }

    // Background traffic over pairs whose recipient already listens
    let listening = Arc::new(AtomicUsize::new(0));
    let sent = Arc::new(AtomicUsize::new(0));
    let sender = (rate > 0.0).then(|| {
        let (listening, sent, pairs) = (listening.clone(), sent.clone(), pairs.clone());
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs_f64(1.0 / rate));
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut n = 0usize;
            loop {
                tick.tick().await;
                // Users 2i and 2i+1 listen once the count passes 2i+1 (resp. 2i)
                let listening = listening.load(Ordering::Relaxed);
                let ready = (listening / 2).max(1);
                let (pair, round) = (n % ready, n / ready);
                n += 1;
                let (a, b, chat_id) = &pairs[pair];
                // Alternate direction, send only towards a listening recipient
                let (from, to_idx) = if round % 2 == 0 {
                    (a, 2 * pair + 1)
                } else {
                    (b, 2 * pair)
                };
                if to_idx >= listening {
                    continue;
                }
                if send_one(from, *chat_id, msg_size).await {
                    from.outbound_service().run_once().await;
                    sent.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
    });

    // Keep all handles alive until the end
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    let mut points: Vec<(f64, f64)> = Vec::new();
    println!(
        "rate={rate} backlog={backlog} slow_every={slow_every} msg_size={msg_size} big_once={big_once}"
    );
    println!(
        "{:>10} {:>10} {:>10} {:>8}",
        "requested", "active", "used_MiB", "sent"
    );
    // Users that already got their one big message
    let mut got_big = vec![false; max];
    for step in steps {
        while handles.len() < step {
            let user = &users[handles.len()];
            let (mut stream, responder) = user.listen_queue().await.unwrap();
            let slow = slow_every > 0 && handles.len().is_multiple_of(slow_every);
            handles.push(tokio::spawn(async move {
                if slow {
                    // Hold the stream and responder without polling or acking
                    std::future::pending::<()>().await;
                }
                // Ack only, nothing is processed
                while let Some(item) = stream.next().await {
                    if let Ok(response) = item
                        && let Some(listen_response::Event::Message(message)) = response.event
                    {
                        responder.ack(message.sequence_number + 1).await;
                    }
                }
            }));
            listening.store(handles.len(), Ordering::Relaxed);
        }
        if big_once {
            let start = Instant::now();
            let todo: Vec<usize> = (0..handles.len())
                .filter(|&i| !is_slow(i) && !got_big[i])
                .collect();
            stream::iter(todo.iter().map(|&i| {
                let (a, b, chat_id) = &pairs[i / 2];
                // The partner of user i sends
                (if i % 2 == 0 { b.clone() } else { a.clone() }, *chat_id)
            }))
            .map(|(from, chat_id)| {
                tokio::spawn(async move {
                    if send_one(&from, chat_id, msg_size).await {
                        from.outbound_service().run_once().await;
                    }
                })
            })
            .buffer_unordered(concurrency)
            .for_each(|joined| async { joined.unwrap() })
            .await;
            for i in todo {
                got_big[i] = true;
            }
            println!("big messages sent in {:.1?}", start.elapsed());
        }
        sleep(settle).await;
        let (mut used, mut active) = (0.0, 0.0);
        for i in 0..SAMPLES {
            if i > 0 {
                sleep(SAMPLE_INTERVAL).await;
            }
            let (u, a) = scrape(&metrics_url).await;
            used += u / SAMPLES as f64;
            active += a / SAMPLES as f64;
        }
        println!(
            "{step:>10} {active:>10.0} {:>10.2} {:>8}",
            used / (1024.0 * 1024.0),
            sent.load(Ordering::Relaxed)
        );
        points.push((active, used));
    }

    if let Some(sender) = sender {
        sender.abort();
    }

    // Least-squares fit of used bytes vs active connections
    let n = points.len() as f64;
    let (sx, sy) = points
        .iter()
        .fold((0.0, 0.0), |(sx, sy), (x, y)| (sx + x, sy + y));
    let sxx: f64 = points.iter().map(|(x, _)| x * x).sum();
    let sxy: f64 = points.iter().map(|(x, y)| x * y).sum();
    let denom = n * sxx - sx * sx;
    if denom.abs() < f64::EPSILON {
        eprintln!("not enough distinct connection counts for a fit");
    } else {
        let slope = (n * sxy - sx * sy) / denom;
        let base = (sy - slope * sx) / n;
        println!(
            "fit: base {:.2} MiB, {:.2} KiB per connection",
            base / (1024.0 * 1024.0),
            slope / 1024.0
        );
    }
    drop(handles);
}
