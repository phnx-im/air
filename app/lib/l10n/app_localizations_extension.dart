// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math';

import 'app_localizations.dart';

extension AppLocalizationsExtension on AppLocalizations {
  String bytesToHumanReadable(int bytes) {
    final List<String> byteUnits = [
      byteUnit_B,
      byteUnit_KB,
      byteUnit_MB,
      byteUnit_GB,
      byteUnit_TB,
      byteUnit_PB,
      byteUnit_EB,
      byteUnit_ZB,
      byteUnit_YB,
    ];

    if (bytes == 0) {
      return attachmentSize(0, byteUnit_B);
    }

    int i = (log(bytes) / log(1000)).floor();
    i = min(i, byteUnits.length - 1);
    double value = bytes / pow(1000, i);

    return attachmentSize(value, byteUnits[i]);
  }
}
