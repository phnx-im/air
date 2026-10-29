// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:uuid/uuid.dart';

import '../helpers.dart';

void main() {
  group('UiEventMessage.isDeletable', () {
    const username = UiUsername(plaintext: 'alice');

    test(
      'keeps device messages, the self chat intro and actorless changes',
      () {
        final deviceId = UuidValue.fromString(
          '00000000-0000-4000-8000-000000000001',
        );
        for (final message in [
          UiSystemMessage.deviceLinked(deviceId),
          UiSystemMessage.deviceUnlinked(deviceId),
          const UiSystemMessage.selfChatCreated(),
          UiSystemMessage.add(null, 2.userId()),
          UiSystemMessage.remove(null, 2.userId()),
        ]) {
          for (final openRequest in [false, true]) {
            expect(
              UiEventMessage.system(message)
                  .isDeletable(openRequest: openRequest),
              isFalse,
              reason: '$message',
            );
          }
        }
      },
    );

    test('keeps request messages while the request is open', () {
      for (final message in [
        UiSystemMessage.receivedHandleConnectionRequest(
          sender: 1.userId(),
          username: username,
        ),
        UiSystemMessage.receivedDirectConnectionRequest(
          sender: 1.userId(),
          groupChat: UiRequestGroupChat.chat(3.chatId()),
        ),
        UiSystemMessage.receivedAdditionalUsernameConnectionRequest(
          sender: 1.userId(),
          username: username,
        ),
        UiSystemMessage.receivedAdditionalDirectConnectionRequest(
          sender: 1.userId(),
          groupChat: UiRequestGroupChat.chat(3.chatId()),
        ),
        const UiSystemMessage.newHandleConnectionChat(username),
        UiSystemMessage.newDirectConnectionChat(1.userId()),
        UiSystemMessage.sentDirectConnectionRequest(
          recipient: 1.userId(),
          originChatId: 3.chatId(),
        ),
      ]) {
        final event = UiEventMessage.system(message);
        expect(
          event.isDeletable(openRequest: true),
          isFalse,
          reason: '$message',
        );
        expect(
          event.isDeletable(openRequest: false),
          isTrue,
          reason: '$message',
        );
      }
    });

    test('deletes other event messages', () {
      for (final event in [
        UiEventMessage.system(UiSystemMessage.add(1.userId(), 2.userId())),
        UiEventMessage.system(UiSystemMessage.remove(1.userId(), 2.userId())),
        UiEventMessage.system(
          UiSystemMessage.acceptedConnectionRequest(
            sender: 1.userId(),
            username: null,
          ),
        ),
        const UiEventMessage.system(UiSystemMessage.connectionRequestExpired()),
        const UiEventMessage.system(UiSystemMessage.onboarded()),
        const UiEventMessage.error(UiErrorMessage(message: 'error')),
      ]) {
        expect(event.isDeletable(openRequest: false), isTrue, reason: '$event');
      }
    });
  });
}
