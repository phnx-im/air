// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/you/rust_licenses.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('rust licenses load from the bundled asset', () async {
    final entries = await rustLicenses().toList();
    final crates = entries.expand((e) => e.packages).toSet();

    expect(crates, contains('tokio'));
    expect(crates, contains('openmls'));
    // Workspace crates are not third party.
    expect(crates, isNot(contains('airapplogic')));
    expect(
      entries.any((e) => e.paragraphs.any((p) => p.text.contains('MIT'))),
      isTrue,
    );
  });
}
