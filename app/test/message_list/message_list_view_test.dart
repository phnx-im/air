// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/message_list/message_list.dart';
import 'package:test/test.dart';

import '../helpers.dart';

void main() {
  group('MessageListView', () {
    test('VisibilityKeyValue equality', () {
      final a = VisibilityKeyValue(1.messageId());
      final b = VisibilityKeyValue(1.messageId());
      expect(a, equals(b));

      final c = VisibilityKeyValue(1.messageId());
      final d = VisibilityKeyValue(2.messageId());
      expect(c, isNot(equals(d)));
    });
  });
}
