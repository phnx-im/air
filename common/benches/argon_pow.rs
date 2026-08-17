// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use aircommon::pow::{PoWConfig, find_nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::Rng;

const DATA: &[u8] = b"benchmark data";
const MEM_COST: u32 = 47104;

/// Benchmark raw Argon2id for a fixed mem & parallelism,
/// sweeping over different time_costs.
fn bench_argon2_time(c: &mut Criterion) {
    let mut group = c.benchmark_group("argon2_time_cost");
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(5))
        .throughput(Throughput::Bytes(DATA.len() as u64));

    let salt = b"PoW_salt_";

    for &time_cost in &[1u32, 128, 512] {
        // build a fresh hasher per cost
        let params = Params::new(MEM_COST, time_cost, 1, Some(8)).unwrap();
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut hash8 = [0u8; 8];

        group.bench_with_input(
            BenchmarkId::from_parameter(time_cost),
            &time_cost,
            |b, &_tc| {
                b.iter(|| {
                    argon2.hash_password_into(DATA, salt, &mut hash8).unwrap();
                })
            },
        );
    }

    group.finish();
}

/// Benchmark full PoW checks at low difficulties
fn bench_pow_difficulty(c: &mut Criterion) {
    let mut group = c.benchmark_group("pow_low_difficulty");
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(5));

    for &difficulty in &[1u64, 16, 64] {
        let cfg = PoWConfig::new(
            difficulty, /* mem_cost */ MEM_COST, /* time_cost */ 1,
            /* parallelism */ 1,
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(difficulty),
            &difficulty,
            |b, &_d| {
                b.iter(|| {
                    let mut salt = [0u8; 16];
                    rand::rng().fill_bytes(&mut salt);
                    find_nonce(DATA, &salt, &cfg);
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_argon2_time, bench_pow_difficulty,);
criterion_main!(benches);
