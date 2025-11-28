// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:visibility_detector/visibility_detector.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/message_list.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';

import '../chat_list/chat_list_content_test.dart';
import '../helpers.dart';
import '../mocks.dart';

const Size _testSize = Size(1080, 2800);

final _chatId = 1.chatId();

final _mobileMessages = [
  UiChatMessage(
    id: 1.messageId(),
    chatId: _chatId,
    timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: "Hello Alice, it's Bob",
          topicId: Uint8List(0),
          content: simpleMessage("Hello Alice, it's Bob"),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 2.messageId(),
    chatId: _chatId,
    timestamp: DateTime.parse('2023-01-01T00:01:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'Hey Bob, are you around later today?',
          topicId: Uint8List(0),
          content: simpleMessage('Hey Bob, are you around later today?'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.start,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 3.messageId(),
    chatId: _chatId,
    timestamp: DateTime.parse('2023-01-01T00:02:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: true,
        content: UiMimiContent(
          plainBody: 'Hello Bob and Eve',
          topicId: Uint8List(0),
          content: simpleMessage('Hello Bob and Eve'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.middle,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 4.messageId(),
    chatId: _chatId,
    timestamp: DateTime.parse('2023-01-01T00:03:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'How are you doing?',
          topicId: Uint8List(0),
          content: simpleMessage('How are you doing?'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.end,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 5.messageId(),
    chatId: _chatId,
    timestamp: DateTime.parse('2023-01-01T00:03:30.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: "Following up with a quick summary of today's meeting.",
          topicId: Uint8List(0),
          content: simpleMessage(
            "Following up with a quick summary of today's meeting.",
          ),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.delivered,
  ),
  UiChatMessage(
    id: 6.messageId(),
    chatId: _chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'Thanks everyone! Talk to you tomorrow.',
          topicId: Uint8List(0),
          content: simpleMessage('Thanks everyone! Talk to you tomorrow.'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.read,
  ),
];

MessageCubit _createMockMessageCubit({
  required UserCubit userCubit,
  required MessageState initialState,
}) => MockMessageCubit(initialState: initialState);

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
  });

  group('Mobile message actions', () {
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
      when(
        () => messageListCubit.state,
      ).thenReturn(MockMessageListState(_mobileMessages));
    });

    Widget buildSubject() => RepositoryProvider<AttachmentsRepository>.value(
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
            final theme = themeData(
              MediaQuery.platformBrightnessOf(context),
            ).copyWith(platform: TargetPlatform.android);
            return MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: theme,
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

    testWidgets('shows overlay on long press', (tester) async {
      tester.view.physicalSize = _testSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      VisibilityDetectorController.instance.updateInterval = Duration.zero;

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      final messageFinder = find.text("Hello Alice, it's Bob");
      expect(messageFinder, findsOneWidget);

      await tester.longPress(messageFinder);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_mobile_actions.png'),
      );
    });
  });
}
