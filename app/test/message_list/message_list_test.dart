// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/chat/chat_details.dart';
import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/message_list.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../helpers.dart';
import '../mocks.dart';

// NB: do not forget to adjust this, when you add more content to render
const highTestSize = Size(1080, 4100);

final chatId = 1.chatId();

final firstMessageContent = UiMimiContent(
  plainBody: 'Hello Alice from Bob',
  topicId: Uint8List(0),
  content: simpleMessage('Hello Alice from Bob'),
  attachments: [],
);

final firstDeletedMessageContent = UiMimiContent(
  topicId: Uint8List(0),
  attachments: [],
  replaces: Uint8List(0),
);

final veryLongMimiContent = UiMimiContent(
  topicId: Uint8List(0),
  plainBody: '''Nice to see you both here! 👋

This is a message with multiple lines. It should be properly displayed in the message bubble and split between multiple lines.''',
  content: simpleMessage(
    '''Nice to see you both here! 👋

This is a message with multiple lines. It should be properly displayed in the message bubble and split between multiple lines.''',
  ),
  attachments: [],
);

final messages = [
  UiChatMessage(
    id: 1.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
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
    timestamp: DateTime.parse('2023-01-01T00:01:00.000Z'),
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
    position: UiFlightPosition.start,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 100.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:01.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: false,
        content: richContent,
      ),
    ),
    position: UiFlightPosition.end,
    status: UiMessageStatus.delivered,
  ),
  UiChatMessage(
    id: 3.messageId(),
    chatId: chatId,
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
    position: UiFlightPosition.start,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 4.messageId(),
    chatId: chatId,
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
    position: UiFlightPosition.middle,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 5.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:03:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: veryLongMimiContent,
      ),
    ),
    position: UiFlightPosition.end,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 7.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:01.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: richContent,
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.delivered,
  ),
  UiChatMessage(
    id: 8.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:01.000Z'),
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
    id: 9.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:03.000Z'),
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
  UiChatMessage(
    id: 10.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:04:04.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "This is a reply to Bob",
          content: simpleMessage("Hello Bob from Alice"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 1.messageId(),
      sender: 1.userId(),
      mimiContent: firstMessageContent,
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.read,
  ),
  UiChatMessage(
    id: 11.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:05:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: firstDeletedMessageContent,
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  UiChatMessage(
    id: 12.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:05:05.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "Bob, wrong chat",
          content: simpleMessage("Bob, wrong chat"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 11.messageId(),
      sender: 2.userId(),
      mimiContent: firstDeletedMessageContent,
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.read,
  ),
  UiChatMessage(
    id: 13.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:05:07.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 3.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "This is an answer to a message I deleted locally",
          content: simpleMessage(
            "This is an answer to a message I deleted locally",
          ),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: const UiInReplyToMessage.notFound(),
    position: UiFlightPosition.single,
    status: UiMessageStatus.read,
  ),
];

final richContent = UiMimiContent(
  topicId: Uint8List(0),
  plainBody: "This is a message with a link https://example.com",
  content: const MessageContent(
    elements: [
      RangedBlockElement(
        start: 0,
        end: 0,
        element: BlockElement_Paragraph([
          RangedInlineElement(
            start: 0,
            end: 0,
            element: InlineElement_Text("This is a rich content message "),
          ),
          RangedInlineElement(
            start: 0,
            end: 0,
            element: InlineElement_Link(
              destUrl: "https://example.com",
              children: [
                RangedInlineElement(
                  start: 0,
                  end: 0,
                  element: InlineElement_Text("https://example.com"),
                ),
              ],
            ),
          ),
        ]),
      ),
      RangedBlockElement(
        start: 0,
        end: 0,
        element: BlockElement_Quote([
          RangedBlockElement(
            start: 0,
            end: 0,
            element: BlockElement_Paragraph([
              RangedInlineElement(
                start: 0,
                end: 0,
                element: InlineElement_Text("This is a quote "),
              ),
              RangedInlineElement(
                start: 0,
                end: 0,
                element: InlineElement_Link(
                  destUrl: "https://example.com",
                  children: [
                    RangedInlineElement(
                      start: 0,
                      end: 0,
                      element: InlineElement_Text("https://example.com"),
                    ),
                  ],
                ),
              ),
            ]),
          ),
        ]),
      ),
    ],
  ),
  attachments: [],
);

final jumboEmojiMessages = [
  // Jumbo: single emoji from other user
  UiChatMessage(
    id: 20.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: '😀',
          topicId: Uint8List(0),
          content: simpleMessage('😀'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  // Jumbo: multiple emoji from self
  UiChatMessage(
    id: 21.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:11:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: '🎉🥳🎊',
          topicId: Uint8List(0),
          content: simpleMessage('🎉🥳🎊'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  // Not jumbo: emoji + text (should keep bubble)
  UiChatMessage(
    id: 22.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:12:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: '😀 hello',
          topicId: Uint8List(0),
          content: simpleMessage('😀 hello'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  // Not jumbo: edited emoji-only (should keep bubble)
  UiChatMessage(
    id: 23.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:13:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: true,
        content: UiMimiContent(
          plainBody: '👍',
          topicId: Uint8List(0),
          content: simpleMessage('👍'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  // Normal text message for contrast
  UiChatMessage(
    id: 24.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:14:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          plainBody: 'Nice!',
          topicId: Uint8List(0),
          content: simpleMessage('Nice!'),
          attachments: [],
        ),
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
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
    timestamp: DateTime.parse('2023-01-01T00:04:00.000Z'),
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
    timestamp: DateTime.parse('2023-01-01T00:04:01.000Z'),
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
    timestamp: DateTime.parse('2023-01-01T00:04:02.000Z'),
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
    timestamp: DateTime.parse('2023-01-01T00:04:03.000Z'),
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

final replyMessages = [
  // Long reply, short message
  UiChatMessage(
    id: 20.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:00.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "Ok!",
          content: simpleMessage("Ok!"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 1.messageId(),
      sender: 3.userId(),
      mimiContent: veryLongMimiContent,
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  // Short reply, long message
  UiChatMessage(
    id: 21.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:01.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: veryLongMimiContent,
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 2.messageId(),
      sender: 2.userId(),
      mimiContent: UiMimiContent(
        topicId: Uint8List(0),
        plainBody: "Hi!",
        content: simpleMessage("Hi!"),
        attachments: [],
      ),
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  // Reply and a single emoji message
  UiChatMessage(
    id: 22.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:02.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 2.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "👍",
          content: simpleMessage("👍"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 1.messageId(),
      sender: 3.userId(),
      mimiContent: firstMessageContent,
    ),
    position: UiFlightPosition.single,
    status: UiMessageStatus.sent,
  ),
  // Reply containing only emoji and some message
  UiChatMessage(
    id: 23.messageId(),
    chatId: chatId,
    timestamp: DateTime.parse('2023-01-01T00:10:03.000Z'),
    message: UiMessage_Content(
      UiContentMessage(
        sender: 1.userId(),
        sent: true,
        edited: false,
        content: UiMimiContent(
          topicId: Uint8List(0),
          plainBody: "That was exactly my reaction!",
          content: simpleMessage("That was exactly my reaction!"),
          attachments: [],
        ),
      ),
    ),
    inReplyToMessage: UiInReplyToMessage.resolved(
      messageId: 3.messageId(),
      sender: 2.userId(),
      mimiContent: UiMimiContent(
        topicId: Uint8List(0),
        plainBody: "🎉🎊✨",
        content: simpleMessage("🎉🎊✨"),
        attachments: [],
      ),
    ),
    position: UiFlightPosition.single,
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
    registerFallbackValue(0.attachmentId());
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
              theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
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
      messageListCubit.setState(MockMessageListState([]));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_empty.png'),
      );
    });

    testWidgets('renders correctly', (tester) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(MockMessageListState(messages));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list.png'),
      );
    });

    testWidgets('renders correctly (dark mode)', (tester) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(MockMessageListState(messages));

      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(() {
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_dark_mode.png'),
      );
    });

    testWidgets('renders correctly with attachments', (tester) async {
      tester.view.physicalSize = const Size(1080, 2400);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(MockMessageListState(attachmentMessages));
      when(
        () => attachmentsRepository.loadImageAttachment(
          attachmentId: any(named: 'attachmentId'),
          chunkEventCallback: any(named: "chunkEventCallback"),
        ),
      ).thenAnswer((_) async => Future.any([]));
      when(
        () => attachmentsRepository.statusStream(
          attachmentId: any(named: 'attachmentId'),
        ),
      ).thenAnswer((_) => Stream.value(const UiAttachmentStatus.completed()));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_attachments.png'),
      );
    });

    testWidgets('renders correctly with blocked messages', (tester) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      final messageWithBobBlocked = [
        for (final message in messages)
          switch (message.message) {
            UiMessage_Content(field0: final content)
                when content.sender == 2.userId() =>
              message.copyWith(status: UiMessageStatus.hidden),
            _ => message,
          },
      ];
      messageListCubit.setState(MockMessageListState(messageWithBobBlocked));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_blocked.png'),
      );
    });

    testWidgets('renders correctly with blocked messages in contact chat', (
      tester,
    ) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      final messageWithBobBlocked = [
        for (final message in messages) ...[
          if (message.sender == 1.userId()) message,
          if (message.sender == 2.userId())
            message.copyWith(status: UiMessageStatus.hidden),
        ],
      ];
      messageListCubit.setState(
        MockMessageListState(messageWithBobBlocked, isConnectionChat: true),
      );

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_blocked_contact_chat.png'),
      );
    });

    testWidgets('renders jumbo emoji without bubble', (tester) async {
      tester.view.physicalSize = const Size(1080, 1350);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(MockMessageListState(jumboEmojiMessages));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_jumbo_emoji.png'),
      );
    });

    testWidgets('renders correctly with disabled read receipts', (
      tester,
    ) async {
      tester.view.physicalSize = highTestSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(MockMessageListState(messages));
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(readReceipts: false));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_disabled_read_receipts.png'),
      );
    });

    testWidgets('renders unread divider', (tester) async {
      tester.view.physicalSize = const Size(1080, 2400);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      // Use a small subset so the golden stays compact.
      // Place the unread divider at index 2, which is mid-flight for
      // Eve (indices 1=start, 2=end). The divider should break the
      // flight: 1→single | divider | 2→single.
      final unreadMessages = [
        for (final (i, msg) in messages.take(6).indexed)
          switch (i) {
            1 => msg.copyWith(position: UiFlightPosition.single),
            2 => msg.copyWith(position: UiFlightPosition.single),
            _ => msg,
          },
      ];

      messageListCubit.setState(
        MockMessageListState(unreadMessages, firstUnreadIndex: 2),
      );

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_unread_divider.png'),
      );
    });

    testWidgets('scrollToMessage loads and reaches an unloaded target', (
      tester,
    ) async {
      // Small viewport so most messages are off-screen.
      tester.view.physicalSize = const Size(400, 600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final manyMessages = List.generate(30, (i) {
        return UiChatMessage(
          id: (200 + i).messageId(),
          chatId: chatId,
          timestamp: DateTime(2023, 1, 1, 0, i),
          message: UiMessage_Content(
            UiContentMessage(
              sender: 2.userId(),
              sent: true,
              edited: false,
              content: UiMimiContent(
                plainBody: 'Message number $i',
                topicId: Uint8List(0),
                content: simpleMessage('Message number $i'),
                attachments: [],
              ),
            ),
          ),
          position: UiFlightPosition.single,
          status: UiMessageStatus.sent,
        );
      });

      final targetMessage = UiChatMessage(
        id: 999.messageId(),
        chatId: chatId,
        timestamp: DateTime(2023, 1, 1, 2, 0),
        message: UiMessage_Content(
          UiContentMessage(
            sender: 2.userId(),
            sent: true,
            edited: false,
            content: UiMimiContent(
              plainBody: 'Loaded around target',
              topicId: Uint8List(0),
              content: simpleMessage('Loaded around target'),
              attachments: [],
            ),
          ),
        ),
        position: UiFlightPosition.single,
        status: UiMessageStatus.sent,
      );

      MessageId? requestedMessageId;
      messageListCubit = MockMessageListCubit(
        initialState: MockMessageListState(manyMessages),
        onJumpToMessage: (messageId) async {
          requestedMessageId = messageId;
          messageListCubit.setState(
            MockMessageListState([...manyMessages, targetMessage]),
          );
        },
      );

      await tester.pumpWidget(buildSubject());

      messageListCubit.emitCommand(
        MessageListCommand.scrollToId(messageId: targetMessage.id),
      );
      await tester.pump();

      // Pump enough frames for the iterative scroll to converge.
      for (var i = 0; i < 15; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }

      expect(requestedMessageId, targetMessage.id);
      expect(find.text('Loaded around target'), findsOneWidget);
    });

    testWidgets('marks the current visible message as read while scrolling', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 600);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final manyMessages = List.generate(30, (i) {
        return UiChatMessage(
          id: (400 + i).messageId(),
          chatId: chatId,
          timestamp: DateTime(2023, 1, 1, 0, i),
          message: UiMessage_Content(
            UiContentMessage(
              sender: 2.userId(),
              sent: true,
              edited: false,
              content: UiMimiContent(
                plainBody: 'Read marker message $i',
                topicId: Uint8List(0),
                content: simpleMessage('Read marker message $i'),
                attachments: [],
              ),
            ),
          ),
          position: UiFlightPosition.single,
          status: UiMessageStatus.sent,
        );
      });

      messageListCubit.setState(MockMessageListState(manyMessages));

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      reset(chatDetailsCubit);
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: 'untilMessageId'),
          untilTimestamp: any(named: 'untilTimestamp'),
        ),
      ).thenAnswer((_) => Future.value());

      tester
          .state<ScrollableState>(find.byType(Scrollable))
          .position
          .jumpTo(250);
      await tester.pump();

      verify(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: 'untilMessageId'),
          untilTimestamp: any(named: 'untilTimestamp'),
        ),
      ).called(1);
    });

    testWidgets('renders correctly with replies of various sizes', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(1080, 3000);
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      messageListCubit.setState(MockMessageListState(replyMessages));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/message_list_with_replies.png'),
      );
    });
  });
}
