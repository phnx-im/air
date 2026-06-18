<!--
SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>

SPDX-License-Identifier: CC-BY-SA-4.0
-->

# APQMLS - Amortized Post-Quantum MLS Combiner

Implementation of the [MLS
Combiner](https://www.ietf.org/archive/id/draft-ietf-mls-combiner-02.html)
(draft-ietf-mls-combiner-02) -- an amortized post-quantum MLS combiner built on
top of [OpenMLS](https://github.com/openmls/openmls).

MLS is a secure group messaging protocol. The MLS Combiner runs a classical and
a post-quantum MLS group in parallel and combines their key material to provide
post-quantum security. The "amortized" aspect batches the post-quantum
operations across multiple epochs, reducing per-message overhead.

## License

AGPL-3.0-or-later
