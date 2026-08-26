// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/share/staged_share.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('dispatchSharedIntoChat', () {
    test('forwards a share when all attachments were dropped', () async {
      final shares = <StagedShare>[];
      final controller = StreamController<StagedShare>(sync: true);
      controller.stream.listen(shares.add);

      dispatchSharedIntoChat({'dropped': 2}, controller.sink);

      expect(shares, hasLength(1));
      expect(shares.single.attachments, isEmpty);
      expect(shares.single.text, isNull);
      expect(shares.single.droppedAttachments, 2);
      await controller.close();
    });

    test('ignores an empty payload', () async {
      final shares = <StagedShare>[];
      final controller = StreamController<StagedShare>(sync: true);
      controller.stream.listen(shares.add);

      dispatchSharedIntoChat(const {}, controller.sink);

      expect(shares, isEmpty);
      await controller.close();
    });
  });
}
