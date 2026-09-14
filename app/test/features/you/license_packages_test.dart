// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/you/license_packages.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

LicenseEntry _entry(List<String> packages, String text) =>
    LicenseEntryWithLineBreaks(packages, text);

void main() {
  test('groups entries by package and sorts case-insensitively', () async {
    final packages = await collectLicensePackages(
      Stream.fromIterable([
        _entry(['zeroize'], 'MIT'),
        _entry(['aircommon', 'airprotos'], 'AGPL-3.0-or-later'),
        _entry(['Flutter'], 'BSD-3-Clause'),
        _entry(['aircommon'], 'Apache-2.0'),
      ]),
    );

    expect(packages.map((package) => package.name), [
      'aircommon',
      'airprotos',
      'Flutter',
      'zeroize',
    ]);
    expect(packages.first.entries, hasLength(2));
    expect(packages.last.entries, hasLength(1));
  });

  test('keeps what was collected before an error', () async {
    final packages = await collectLicensePackages(
      Stream.fromIterable([
        _entry(['zeroize'], 'MIT'),
      ]).asyncExpand(
        (entry) => Stream.fromFutures([
          Future.value(entry),
          Future.error(StateError('missing asset')),
        ]),
      ),
    );

    expect(packages.map((package) => package.name), ['zeroize']);
  });
}
