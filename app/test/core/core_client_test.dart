// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../helpers.dart';
import '../mocks.dart';

void main() {
  group('CoreClient.deleteUnloadedDatabase', () {
    tearDown(() => CoreClient().logout());

    test('refuses to delete the database of the loaded user', () async {
      final user = MockUser();
      when(() => user.clientRecordId).thenReturn(1.clientRecordId());
      CoreClient().user = user;

      await expectLater(
        CoreClient().deleteUnloadedDatabase(1.clientRecordId()),
        throwsStateError,
      );
    });
  });
}
