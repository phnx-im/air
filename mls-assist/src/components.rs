// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::component::ComponentId;
use tls_codec::{TlsDeserialize, TlsDeserializeBytes, TlsSerialize, TlsSize};

/// A list of components as defined in the [Draft-IETF-MLS-Extensions], section 5.
///
/// ```text
/// struct {
///     ComponentID component_ids<V>;
/// } ComponentsList;
/// ```
///
/// [Draft-IETF-MLS-Extensions]: https://www.ietf.org/archive/id/draft-ietf-mls-extensions-09.html
#[derive(Debug, TlsSerialize, TlsDeserialize, TlsDeserializeBytes, TlsSize)]
pub struct ComponentsList {
    pub component_ids: Vec<ComponentId>,
}
