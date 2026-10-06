// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

const FIBONACCI: [Duration; 16] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(3),
    Duration::from_secs(5),
    Duration::from_secs(8),
    Duration::from_secs(13),
    Duration::from_secs(21),
    Duration::from_secs(34),
    Duration::from_secs(55), // index 8
    Duration::from_secs(89),
    Duration::from_secs(144),
    Duration::from_secs(233),
    Duration::from_secs(377),
    Duration::from_secs(610),
    Duration::from_secs(987),
    Duration::from_secs(1597), // 26 min
];

const BACKOFFS_LEN: usize = 9;
const BACKOFFS_EXTENDED_LEN: usize = FIBONACCI.len();

pub struct FibonacciBackoff {
    current_idx: usize,
    is_extended: bool,
}

impl FibonacciBackoff {
    pub(crate) fn new() -> Self {
        FibonacciBackoff {
            current_idx: 0,
            is_extended: false,
        }
    }

    #[allow(
        dead_code,
        reason = "will be used when we detect the device is offline"
    )]
    pub(crate) fn new_extended() -> Self {
        FibonacciBackoff {
            current_idx: 0,
            is_extended: true,
        }
    }

    #[must_use]
    pub(crate) fn next_backoff(&mut self) -> Duration {
        let backoff = FIBONACCI[self.current_idx];
        if self.current_idx + 1 < self.len() {
            self.current_idx += 1;
        }
        backoff
    }

    fn len(&mut self) -> usize {
        if self.is_extended {
            BACKOFFS_EXTENDED_LEN
        } else {
            BACKOFFS_LEN
        }
    }

    pub(crate) fn reset(&mut self) {
        self.current_idx = 0;
        self.is_extended = false;
    }

    #[allow(
        dead_code,
        reason = "will be used when we detect the device is offline"
    )]
    pub(crate) fn reset_extended(&mut self) {
        self.current_idx = 0;
        self.is_extended = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_backoff() {
        let mut backoff = FibonacciBackoff::new();
        let mut a = 1;
        let mut b = 2;
        for _ in 0..9 {
            let timeout = backoff.next_backoff();
            assert_eq!(timeout.as_secs(), a);
            let c = a + b;
            a = b;
            b = c;
        }
        assert_eq!(backoff.next_backoff(), Duration::from_secs(55));
        assert_eq!(backoff.next_backoff(), Duration::from_secs(55));
    }

    #[test]
    fn test_next_backoff_extended() {
        let mut backoff = FibonacciBackoff::new_extended();
        let mut a = 1;
        let mut b = 2;
        for _ in 0..16 {
            let timeout = backoff.next_backoff();
            assert_eq!(timeout.as_secs(), a);
            let c = a + b;
            a = b;
            b = c;
        }
        assert_eq!(backoff.next_backoff(), Duration::from_secs(1597));
        assert_eq!(backoff.next_backoff(), Duration::from_secs(1597));
    }

    #[test]
    fn test_reset() {
        let mut backoff = FibonacciBackoff::new();
        let mut a = 1;
        let mut b = 2;
        for _ in 0..9 {
            let timeout = backoff.next_backoff();
            assert_eq!(timeout.as_secs(), a);
            let c = a + b;
            a = b;
            b = c;
        }
        backoff.reset();
        assert_eq!(backoff.next_backoff(), Duration::from_secs(1));
        assert_eq!(backoff.next_backoff(), Duration::from_secs(2));
    }
}
