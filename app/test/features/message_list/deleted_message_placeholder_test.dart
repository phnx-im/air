// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/message_list/message_list_view.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/message_list/message_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/ds/patterns/message_meta/message_meta.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../../mocks.dart';

const _testSize = Size(1080, 2800);

final _chatId = 1.chatId();

/// The time a message with [id] carries. The fixtures space their messages far
/// enough apart that each row stands on its own rather than joining its
/// neighbor's group.
DateTime _timestamp(int id) =>
    DateTime.parse('2023-01-01T00:00:00.000Z').add(Duration(minutes: id * 6));

/// Create a deleted message (replaces != null, content == null)
///
/// [edited] models a row deleted before deletions stopped stamping an edit
/// time, which the placeholder must not report either.
UiChatMessage _deletedMessage({
  required int id,
  required int senderId,
  bool edited = false,
}) => UiChatMessage(
  id: id.messageId(),
  chatId: _chatId,
  timestamp: _timestamp(id),
  message: UiMessage_Content(
    UiContentMessage(
      sender: senderId.userId(),
      sent: true,
      edited: edited,
      content: UiMimiContent(
        replaces: Uint8List.fromList([1, 2, 3, 4]), // Non-null marks as deleted
        topicId: Uint8List(0),
        content: null, // null content indicates message was deleted
        attachments: [],
      ),
    ),
  ),
  status: UiMessageStatus.deleted,
  reactions: [],
);

/// Create a regular text message
UiChatMessage _textMessage({
  required int id,
  required int senderId,
  required String text,
}) => UiChatMessage(
  id: id.messageId(),
  chatId: _chatId,
  timestamp: _timestamp(id),
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
  status: UiMessageStatus.sent,
  reactions: [],
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
        _textMessage(id: 1, senderId: 2, text: 'Hello!'),
        // 2. Regular message from me
        _textMessage(id: 2, senderId: 1, text: 'Hi there!'),
        // 3. Deleted message from me - "You deleted this message."
        _deletedMessage(id: 3, senderId: 1),
        // 4. Deleted message from them (Bob) - "Bob deleted this message."
        _deletedMessage(id: 4, senderId: 2),
        // 5. Regular message from me
        _textMessage(id: 5, senderId: 1, text: 'See you later!'),
      ];

      messageListCubit.setState(messages, isConnectionChat: true);

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
        _textMessage(id: 1, senderId: 3, text: 'Hey everyone!'),
        // 2. Regular message from me
        _textMessage(id: 2, senderId: 1, text: 'Hi Eve!'),
        // 3. Deleted message from me - "You deleted this message."
        _deletedMessage(id: 3, senderId: 1),
        // 4. Deleted message from Bob - "Bob deleted this message."
        _deletedMessage(id: 4, senderId: 2),
        // 5. Regular message from Eve
        _textMessage(id: 5, senderId: 3, text: 'What happened?'),
      ];

      messageListCubit.setState(messages);

      await tester.pumpWidget(buildSubject(messages, isConnectionChat: false));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/deleted_message_group.png'),
      );
    });

    testWidgets('keeps the time but reports no edit or delivery state', (
      tester,
    ) async {
      tester.view.physicalSize = _testSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      // The newest row and the reader's own last word in the chat, so it is
      // where a delivery state would surface if any did.
      final messages = [
        _textMessage(id: 1, senderId: 2, text: 'Hello!'),
        _deletedMessage(id: 2, senderId: 1, edited: true),
      ];

      messageListCubit.setState(messages, isConnectionChat: true);

      await tester.pumpWidget(buildSubject(messages, isConnectionChat: true));

      // The end of the chat is the one row that still reads as a clock, and a
      // deletion takes the delivery state and the edit marker with it rather
      // than the time. It is also the only row carrying a stamp at all here.
      final stamp = tester.widget<MessageMeta>(find.byType(MessageMeta));
      expect(stamp.timestamp, isNotNull);
      expect(stamp.status, isNull);
      expect(stamp.statusLabel, isNull);
      expect(stamp.editedLabel, isNull);
    });
  });
}
