// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/share/pending_share.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  group('deleteShareFiles', () {
    late Directory cacheDir;

    setUp(() async {
      cacheDir = await Directory.systemTemp.createTemp('pending-share-test');
    });

    tearDown(() => cacheDir.delete(recursive: true));

    Future<File> fileIn(String dir) async {
      final file = File(p.join(cacheDir.path, dir, 'a.txt'));
      await file.create(recursive: true);
      return file;
    }

    test(
      "takes the file's own directory under the share cache with it",
      () async {
        final file = await fileIn('share/uuid');

        await deleteShareFiles([UiSharedAttachment(path: file.path)]);

        expect(await file.parent.exists(), isFalse);
        expect(
          await Directory(p.join(cacheDir.path, 'share')).exists(),
          isTrue,
        );
      },
    );

    test('deletes only the file when it is from anywhere else', () async {
      final file = await fileIn('elsewhere');
      final sibling = await fileIn('elsewhere/sibling');

      await deleteShareFiles([UiSharedAttachment(path: file.path)]);

      expect(await file.exists(), isFalse);
      expect(await sibling.exists(), isTrue);
    });
  });

  group('dispatchSharedIntoChat', () {
    late List<ShareHandoff> handoffs;
    late StreamController<ShareHandoff> controller;

    setUp(() {
      handoffs = [];
      controller = StreamController<ShareHandoff>(sync: true);
      controller.stream.listen(handoffs.add);
    });

    tearDown(() => controller.close());

    test('forwards a share when all attachments were dropped', () {
      dispatchSharedIntoChat({'dropped': 2}, controller.sink);

      expect(handoffs, hasLength(1));
      expect(handoffs.single.share.attachments, isEmpty);
      expect(handoffs.single.share.text, isNull);
      expect(handoffs.single.share.droppedAttachments, 2);
      expect(handoffs.single.chatId, isNull);
    });

    test('ignores an empty payload', () {
      dispatchSharedIntoChat(const {}, controller.sink);

      expect(handoffs, isEmpty);
    });

    test('carries the chat id a direct share target named', () {
      dispatchSharedIntoChat({
        'chatId': '00000000-0000-4000-8000-000000000001',
        'text': 'hello',
      }, controller.sink);

      expect(handoffs, hasLength(1));
      expect(
        handoffs.single.chatId?.uuid.toString(),
        '00000000-0000-4000-8000-000000000001',
      );
    });

    test('degrades to no destination when the chat id is malformed', () {
      dispatchSharedIntoChat({
        'chatId': 'not-a-uuid',
        'text': 'hello',
      }, controller.sink);

      expect(handoffs, hasLength(1));
      expect(handoffs.single.chatId, isNull);
      expect(handoffs.single.share.text, 'hello');
    });

    test('pairs each path with its mime type, empty ones dropped', () {
      dispatchSharedIntoChat({
        'paths': ['/tmp/a.png', '/tmp/b.bin'],
        'mimeTypes': ['image/png', ''],
      }, controller.sink);

      final attachments = handoffs.single.share.attachments;
      expect(attachments.map((a) => a.path), ['/tmp/a.png', '/tmp/b.bin']);
      expect(attachments.map((a) => a.mimeType), ['image/png', null]);
    });
  });
}
