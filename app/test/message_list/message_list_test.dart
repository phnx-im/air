// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/message_list.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:visibility_detector/visibility_detector.dart';

import '../chat_list/chat_list_content_test.dart';
import '../helpers.dart';
import '../mocks.dart';

final chatId = 1.chatId();

final messages = [
  UiChatMessage(
    id: 1.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:00:00.000Z',
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'Hello Alice from Bob',
          topicId: Uint8List(0),
          content: simpleMessage('Hello Alice from Bob'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 2.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:01:00.000Z',
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: true,
        content: UiMimiContent(
          plainBody:
              'Hello Alice. This is a long message that should not be truncated but properly split into multiple lines.',
          topicId: Uint8List(0),
          content: simpleMessage(
            'Hello Alice. This is a long message that should not be truncated but properly split into multiple lines.',
          ),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 3.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:02:00.000Z',
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
    position: UiFlightPosition.start,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 4.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:03:00.000Z',
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
    position: UiFlightPosition.middle,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 5.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:03:00.000Z',
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: '''Nice to see you both here! 👋

This is a message with multiple lines. It should be properly displayed in the message bubble and split between multiple lines.''',
          topicId: Uint8List(0),
          content: simpleMessage(
            '''Nice to see you both here! 👋

This is a message with multiple lines. It should be properly displayed in the message bubble and split between multiple lines.''',
          ),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.end,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 7.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:04:01.000Z',
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "This is a delivered message",
          content: simpleMessage("This is a delivered message"),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.delivered,
  ),
  UiChatMessage(
    id: 8.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:04:02.000Z',
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "This is a read message",
          content: simpleMessage("This is a read message"),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.read,
  ),
];

final imageAttachment = UiAttachment(
  attachmentId: 2.attachmentId(),
  filename: "image.png",
  size: 10 * 1024 * 1024,
  contentType: 'image/png',
  description: "A woman eating a donut",
  imageMetadata: const UiImageMetadata(
    blurhash: "LEHLk~WB2yk8pyo0adR*.7kCMdnj",
    width: 100,
    height: 50,
  ),
);

final attachmentMessages = [
  UiChatMessage(
    id: 6.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:04:00.000Z',
    position: UiFlightPosition.start,
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "A File Attachment",
          content: simpleMessage('A File Attachment'),
          attachments: [
            UiAttachment(
              attachmentId: 1.attachmentId(),
              filename: "file.zip",
              contentType: "application/zip",
              size: 1024,
              description: "Failing golden tests",
            ),
          ],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 7.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:04:01.000Z',
    position: UiFlightPosition.end,
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "Look what I've got to eat",
          content: simpleMessage("Look what I've got to eat"),
          attachments: [imageAttachment],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 8.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:04:02.000Z',
    position: UiFlightPosition.single,
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          attachments: [imageAttachment],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 9.messageId(),
    chatId: chatId,
    timestamp: '2023-01-01T00:04:03.000Z',
    position: UiFlightPosition.single,
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "Small image",
          content: simpleMessage("Small image"),
          attachments: [
            imageAttachment.copyWith(
              imageMetadata: imageAttachment.imageMetadata!.copyWith(
                width: 10,
                height: 10,
              ),
            ),
          ],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
  ),
];

MessageCubit createMockMessageCubit({
  required UserCubit userCubit,
  required MessageState initialState,
}) => MockMessageCubit(initialState: initialState);

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
  });

  group('MessageListView', () {
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockAttachmentsRepository attachmentsRepository;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
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
      ).thenAnswer((_) => Future.value());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
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
            return MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: themeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: const Scaffold(
                body: MessageListView(
                  createMessageCubit: createMockMessageCubit,
                ),
              ),
            );
          },
        ),
      ),
    );

    testWidgets('renders correctly when empty', (tester) async {
      when(() => messageListCubit.state).thenReturn(MockMessageListState([]));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_empty.png'),
      );
    });

    testWidgets('renders correctly', (tester) async {
      when(
        () => messageListCubit.state,
      ).thenReturn(MockMessageListState(messages));

      VisibilityDetectorController.instance.updateInterval = Duration.zero;

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list.png'),
      );
    });

    testWidgets('renders correctly with attachments', (tester) async {
      when(
        () => messageListCubit.state,
      ).thenReturn(MockMessageListState(messages + attachmentMessages));
      when(
        () => attachmentsRepository.loadImageAttachment(
          attachmentId: imageAttachment.attachmentId,
          chunkEventCallback: any(named: "chunkEventCallback"),
        ),
      ).thenAnswer((_) async => Future.any([]));

      VisibilityDetectorController.instance.updateInterval = Duration.zero;

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_attachments.png'),
      );
    });

    testWidgets('renders correctly with blocked messages', (tester) async {
      final messageWithBobBlocked = [
        for (final message in messages)
          switch (message.message) {
            UiMessage_Content(field0: final content)
                when content.sender == 2.userId() =>
              message.copyWith(status: UiMessageStatus.hidden),
            _ => message,
          },
      ];
      when(
        () => messageListCubit.state,
      ).thenReturn(MockMessageListState(messageWithBobBlocked));

      VisibilityDetectorController.instance.updateInterval = Duration.zero;

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_blocked.png'),
      );
    });

    testWidgets('renders correctly with blocked messages in contact chat', (
      tester,
    ) async {
      final messageWithBobBlocked = [
        for (final message in messages) ...[
          if (message.sender == 1.userId()) message,
          if (message.sender == 2.userId())
            message.copyWith(status: UiMessageStatus.hidden),
        ],
      ];
      when(() => messageListCubit.state).thenReturn(
        MockMessageListState(messageWithBobBlocked, isConnectionChat: true),
      );

      VisibilityDetectorController.instance.updateInterval = Duration.zero;

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_blocked_contact_chat.png'),
      );
    });
  });
}
