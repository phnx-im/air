// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math';

extension StringFormatting on String {
  String spacedInGroupsOf(int chunkSize) {
    final chunks = <String>[];
    for (var i = 0; i < length; i += chunkSize) {
      chunks.add(substring(i, min(i + chunkSize, length)));
    }
    return chunks.join(' ');
  }

  String digitsOnly() {
    return replaceAll(RegExp(r'\D'), "");
  }
}
