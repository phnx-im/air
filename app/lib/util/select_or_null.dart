// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';

extension SelectOrNullExtension on BuildContext {
  /// Selects from a [T] above this context, or returns null when there is
  /// none, e.g. where a message renders outside the message list.
  R? selectOrNull<T, R>(R Function(T value) selector) {
    try {
      return select(selector);
    } on ProviderNotFoundException {
      return null;
    }
  }
}
