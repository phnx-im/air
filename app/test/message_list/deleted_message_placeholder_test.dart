// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/message_list.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:visibility_detector/visibility_detector.dart';

import '../chat_list/chat_list_content_test.dart';
import '../helpers.dart';
import '../mocks.dart';

const _testSize = Size(1080, 2800);

final _chatId = 1.chatId();

/// Create a deleted message (replaces != null, content == null)
UiChatMessage _deletedMessage({
  required int id,
  required int senderId,
  required UiFlightPosition position,
}) => UiChatMessage(
  id: id.messageId(),
  chatId: _chatId,
  timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
  message: UiMessage_Content(
    UiContentMessage(
      sender: senderId.userId(),
      sent: true,
      edited: false,
      content: UiMimiContent(
        replaces: Uint8List.fromList([1, 2, 3, 4]), // Non-null marks as deleted
        topicId: Uint8List(0),
        content: null, // null content indicates message was deleted
        attachments: [],
      ),
    ),
  ),
  position: position,
  status: UiMessageStatus.sent,
);

/// Create a regular text message
UiChatMessage _textMessage({
  required int id,
  required int senderId,
  required String text,
  required UiFlightPosition position,
}) => UiChatMessage(
  id: id.messageId(),
  chatId: _chatId,
  timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
  message: UiMessage_Content(
    UiContentMessage(
      sender: senderId.userId(),
      sent: true,
      edited: false,
      content: UiMimiContent(
        topicId: Uint8List(0),
        plainBody: text,
        content: simpleMessage(text),
        attachments: [],
      ),
    ),
  ),
  position: position,
  status: UiMessageStatus.sent,
);

MessageCubit _createMockMessageCubit({
  required UserCubit userCubit,
  required MessageState initialState,
}) => MockMessageCubit(initialState: initialState);

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
  });

  group('Deleted message placeholder', () {
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockAttachmentsRepository attachmentsRepository;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() {
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      attachmentsRepository = MockAttachmentsRepository();
      userSettingsCubit = MockUserSettingsCubit();

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: 'untilMessageId'),
          untilTimestamp: any(named: 'untilTimestamp'),
        ),
      ).thenAnswer((_) async {});
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    });

    Widget buildSubject(
      List<UiChatMessage> messages, {
      bool isConnectionChat = false,
    }) => RepositoryProvider<AttachmentsRepository>.value(
      value: attachmentsRepository,
      child: MultiBlocProvider(
        providers: [
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: contactsCubit),
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<MessageListCubit>.value(value: messageListCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: Builder(
          builder: (context) {
            return MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: const Scaffold(
                body: MessageListView(
                  createMessageCubit: _createMockMessageCubit,
                ),
              ),
            );
          },
        ),
      ),
    );

    testWidgets('renders deleted messages in 1:1 conversation', (tester) async {
      tester.view.physicalSize = _testSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      // 1:1 conversation with mix of regular and deleted messages
      // User 1 = self (Alice), User 2 = Bob
      final messages = [
        // 1. Regular message from them (Bob)
        _textMessage(
          id: 1,
          senderId: 2,
          text: 'Hello!',
          position: UiFlightPosition.single,
        ),
        // 2. Regular message from me
        _textMessage(
          id: 2,
          senderId: 1,
          text: 'Hi there!',
          position: UiFlightPosition.single,
        ),
        // 3. Deleted message from me - "You deleted this message."
        _deletedMessage(id: 3, senderId: 1, position: UiFlightPosition.single),
        // 4. Deleted message from them (Bob) - "Bob deleted this message."
        _deletedMessage(id: 4, senderId: 2, position: UiFlightPosition.single),
        // 5. Regular message from me
        _textMessage(
          id: 5,
          senderId: 1,
          text: 'See you later!',
          position: UiFlightPosition.single,
        ),
      ];

      when(
        () => messageListCubit.state,
      ).thenReturn(MockMessageListState(messages, isConnectionChat: true));

      VisibilityDetectorController.instance.updateInterval = Duration.zero;

      await tester.pumpWidget(buildSubject(messages, isConnectionChat: true));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/deleted_message_1to1.png'),
      );
    });

    testWidgets('renders deleted messages in group conversation', (
      tester,
    ) async {
      tester.view.physicalSize = _testSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      // Group conversation with mix of regular and deleted messages
      // User 1 = self (Alice), User 2 = Bob, User 3 = Eve (acting as "Alice" per userProfiles)
      // Note: userProfiles has: 1=Alice, 2=Bob, 3=Eve
      // We'll use senderId 1 for self messages, and 2/3 for others
      final messages = [
        // 1. Regular message from Alice (user 3 = Eve in userProfiles, but let's use meaningful names)
        // Actually per userProfiles: userId 1 = Alice, 2 = Bob, 3 = Eve
        // Self is userId 1, so messages from others should be 2, 3, etc.
        _textMessage(
          id: 1,
          senderId: 3,
          text: 'Hey everyone!',
          position: UiFlightPosition.single,
        ),
        // 2. Regular message from me
        _textMessage(
          id: 2,
          senderId: 1,
          text: 'Hi Eve!',
          position: UiFlightPosition.single,
        ),
        // 3. Deleted message from me - "You deleted this message."
        _deletedMessage(id: 3, senderId: 1, position: UiFlightPosition.single),
        // 4. Deleted message from Bob - "Bob deleted this message."
        _deletedMessage(id: 4, senderId: 2, position: UiFlightPosition.single),
        // 5. Regular message from Eve
        _textMessage(
          id: 5,
          senderId: 3,
          text: 'What happened?',
          position: UiFlightPosition.single,
        ),
      ];

      when(
        () => messageListCubit.state,
      ).thenReturn(MockMessageListState(messages, isConnectionChat: false));

      VisibilityDetectorController.instance.updateInterval = Duration.zero;

      await tester.pumpWidget(buildSubject(messages, isConnectionChat: false));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/deleted_message_group.png'),
      );
    });
  });
}
