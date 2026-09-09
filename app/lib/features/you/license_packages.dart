// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';

final _log = Logger('LicensePackages');

/// One package and every license entry registered under its name. A package
/// shows up in more than one entry when it ships under several licenses.
class LicensePackage {
  const LicensePackage({required this.name, required this.entries});

  final String name;
  final List<LicenseEntry> entries;
}

/// Groups the registered licenses by package name, sorted case-insensitively.
///
/// Does not touch [LicenseEntry.paragraphs], which is computed lazily and
/// costly for the bundled NOTICES.
Future<List<LicensePackage>> collectLicensePackages(
  Stream<LicenseEntry> licenses,
) async {
  final byPackage = <String, List<LicenseEntry>>{};
  try {
    await for (final entry in licenses) {
      for (final package in entry.packages) {
        (byPackage[package] ??= <LicenseEntry>[]).add(entry);
      }
    }
  } catch (e, st) {
    // Show what was collected before the failing collector.
    _log.severe('Failed to collect licenses', e, st);
  }

  final names = byPackage.keys.toList()
    ..sort((a, b) => a.toLowerCase().compareTo(b.toLowerCase()));
  return [
    for (final name in names)
      LicensePackage(name: name, entries: byPackage[name]!),
  ];
}
