// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Padding for data structures.

/// Returns the len of padded content for the given len of content.
///
/// Based on the Algorithm 1 "Padmé" from [[1]], p. 17.
///
/// [1]: https://bford.info/pub/sec/purb.pdf
pub fn padme_len(len: usize) -> usize {
    // Note: we don't use `ilog2` here, because we want to support len == 0.
    let e = 63u32.saturating_sub(len.leading_zeros()) as usize; // ilog2(len)
    let s = (63u32.saturating_sub(e.leading_zeros()) + 1) as usize; // ilog2(e) + 1
    let z = e.saturating_sub(s);
    let mask = (1usize << z) - 1;
    (len + mask) & !mask
}

/// Returns the length of padding added to the given len of content.
///
/// See [`padme_len`].
pub fn padme_padding_len(len: usize) -> usize {
    padme_len(len).saturating_sub(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padme_len_known_values() {
        assert_eq!(padme_len(0), 0);
        assert_eq!(padme_len(1), 1);
        assert_eq!(padme_len(2), 2);
        assert_eq!(padme_len(3), 3);
        assert_eq!(padme_len(4), 4);
        assert_eq!(padme_len(7), 7);
        assert_eq!(padme_len(8), 8);
        assert_eq!(padme_len(9), 10);
        assert_eq!(padme_len(15), 16);
        assert_eq!(padme_len(16), 16);
        assert_eq!(padme_len(17), 18);
        assert_eq!(padme_len(100), 104);
        assert_eq!(padme_len(128), 128);
        assert_eq!(padme_len(129), 144);
    }

    #[test]
    fn padme_powers_of_two() {
        for exp in 0..usize::BITS - 1 {
            let len = 2usize.pow(exp);
            assert_eq!(padme_len(len), len);
        }
    }

    #[test]
    fn padme_len_is_at_least_len() {
        for len in [
            2, 3, 5, 8, 16, 17, 63, 64, 100, 127, 128, 255, 256, 1000, 65536,
        ] {
            assert!(len <= padme_len(len), "padme_len({len}) > {len}");
        }
    }

    #[test]
    fn padme_padding_len_known_values() {
        assert_eq!(padme_padding_len(2), 0);
        assert_eq!(padme_padding_len(9), 1);
        assert_eq!(padme_padding_len(15), 1);
        assert_eq!(padme_padding_len(17), 1);
        assert_eq!(padme_padding_len(100), 4);
        assert_eq!(padme_padding_len(129), 15);
    }
}
