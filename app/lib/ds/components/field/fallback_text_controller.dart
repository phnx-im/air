// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

/// Gives a field a [TextEditingController] whether or not its host passed one,
/// which is what lets a host that only follows the value through `onChanged`
/// leave the controller out.
///
/// Only the fallback is disposed. A host's controller outlives the field it was
/// handed to, so disposing that one would break the host's next build, and the
/// asymmetry is the whole reason this sits in one place.
mixin FallbackTextController<T extends StatefulWidget> on State<T> {
  /// The controller the host passed, if any. The field reads it off its own
  /// widget, which this mixin cannot see.
  TextEditingController? get hostController;

  TextEditingController? _fallback;

  /// The host's controller, or the one this state owns.
  TextEditingController get controller =>
      hostController ?? (_fallback ??= TextEditingController());

  @override
  void dispose() {
    _fallback?.dispose();
    super.dispose();
  }
}
