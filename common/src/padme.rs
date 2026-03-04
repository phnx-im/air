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
    dbg!(len, e, s);
    let z = e - s;
    let mask = (1usize << z) - 1;
    (len + mask) & !mask
}
