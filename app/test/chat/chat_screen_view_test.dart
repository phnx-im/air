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
import 'package:air/core/lib.dart' show U8Array32;
import 'package:air/l10n/l10n.dart';
import 'package:air/message_list/message_list.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/user/user.dart';

import '../chat_list/chat_list_content_test.dart';
import '../helpers.dart';
import '../message_list/message_list_test.dart';
import '../mocks.dart';

final _chat = chats[2]; // Group chat, isConfirmed = true

final members = [1.userId(), 2.userId(), 3.userId()];

final _navState = NavigationState.home(
  home: HomeNavigationState(chatId: _chat.id),
);

UiChatMessage _msg(int id, String text, {UiUserId? sender}) => UiChatMessage(
  id: id.messageId(),
  chatId: _chat.id,
  timestamp: DateTime.parse('2023-01-01T00:00:00.000Z'),
  message: UiMessage_Content(
    UiContentMessage(
      sender: sender ?? 2.userId(),
      sent: true,
      edited: false,
      content: UiMimiContent(
        plainBody: text,
        topicId: Uint8List(0),
        content: simpleMessage(text),
        attachments: [],
      ),
    ),
  ),
  position: UiFlightPosition.single,
  status: UiMessageStatus.sent,
);

UiChatDetails _chatWithDraft(UiMessageDraft draft) => UiChatDetails(
  id: _chat.id,
  status: _chat.status,
  chatType: _chat.chatType,
  lastUsed: _chat.lastUsed,
  attributes: _chat.attributes,
  messagesCount: _chat.messagesCount,
  unreadMessages: _chat.unreadMessages,
  lastMessage: _chat.lastMessage,
  draft: draft,
);

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
  });

  group('ChatScreenView', () {
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: _chat, members: members));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: "untilMessageId"),
          untilTimestamp: any(named: "untilTimestamp"),
        ),
      ).thenAnswer((_) => Future.value());
      when(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: any(named: "isCommitted"),
        ),
      ).thenAnswer((_) async => Future.value());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      when(() => navigationCubit.state).thenReturn(_navState);
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
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
            home: Scaffold(
              body: ChatScreenView(
                createMessageCubit: createMockMessageCubit,
                textEditingController: TextEditingController(),
              ),
            ),
          );
        },
      ),
    );

    testWidgets('renders correctly when empty', (tester) async {
      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      messageListCubit.setState(const []);

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_screen_empty.png'),
      );
    });

    group('composer states', () {
      // State 1: Empty composer — plus button on the left, no right button.
      testWidgets('empty', (tester) async {
        messageListCubit.setState([
          _msg(1, 'Composer is empty. No text has been entered yet.'),
          _msg(2, 'Only the plus button is visible on the left.'),
        ]);

        await tester.pumpWidget(buildSubject());
        await tester.pump();

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/composer_empty.png'),
        );
      });

      // State 2: Empty composer, scrolled back — plus on the left,
      // scroll-to-bottom chevron on the right.
      testWidgets('empty scrolled back', (tester) async {
        messageListCubit.setState([
          // Reversed list: index 0 = bottom (newest).
          for (int i = 1; i <= 4; i++) _msg(i, 'Old message $i'),
          _msg(5, 'Composer is empty and the user has scrolled up.'),
          _msg(6, 'A scroll-to-bottom button appears on the right.'),
          // Long message near the top — after scrolling it lands
          // right at the composer and gets partially clipped.
          _msg(
            7,
            'This is a long message that should be partially '
            'hidden behind the composer to show that the user '
            'has scrolled back in the conversation history.',
          ),
          for (int i = 8; i <= 12; i++) _msg(i, 'Old message $i'),
        ], hasNewer: true);

        await tester.pumpWidget(buildSubject());
        await tester.pump();
        // Scroll so the explanatory messages are visible and the long
        // message at the bottom is partially hidden by the composer.
        await tester.drag(find.byType(ListView), const Offset(0, 640));
        await tester.pump();

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/composer_empty_scrolled_back.png'),
        );
      });

      // State 3: Unsent text — plus on the left, send arrow on the right.
      testWidgets('unsent', (tester) async {
        messageListCubit.setState([
          _msg(1, 'The user has typed a message but not sent it.'),
          _msg(2, 'A send button appears on the right.'),
        ]);

        await tester.pumpWidget(buildSubject());
        await tester.pump();
        // Enter text and flush the 1-second draft debounce timer.
        await tester.enterText(
          find.byType(TextField),
          'This message has not been sent yet',
        );
        await tester.pump(const Duration(seconds: 1));

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/composer_unsent.png'),
        );
      });

      // State 4: Unsent text, scrolled back — plus on the left,
      // scroll-to-bottom chevron on the right (not send).
      testWidgets('unsent scrolled back', (tester) async {
        messageListCubit.setState([
          for (int i = 1; i <= 4; i++) _msg(i, 'Old message $i'),
          _msg(5, 'The user typed a message and scrolled up.'),
          _msg(6, 'Scroll-to-bottom takes priority over send.'),
          _msg(
            7,
            'This is a long message that should be partially '
            'hidden behind the composer to show that the user '
            'has scrolled back in the conversation history.',
          ),
          for (int i = 8; i <= 12; i++) _msg(i, 'Old message $i'),
        ], hasNewer: true);

        await tester.pumpWidget(buildSubject());
        await tester.pump();
        // Scroll so the explanatory messages are visible and the long
        // message at the bottom fades out through the gradient.
        await tester.drag(find.byType(ListView), const Offset(0, 640));
        await tester.pump();
        // Enter text and flush the 1-second draft debounce timer.
        await tester.enterText(find.byType(TextField), 'Unsent message');
        await tester.pump(const Duration(seconds: 1));
        // Extra pump so the fade resizes after the composer height change.
        await tester.pump();

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/composer_unsent_scrolled_back.png'),
        );
      });

      // State 5: Editing — cancel (X) on the left, confirm (check) on the
      // right. The plus button is replaced by the cancel button.
      testWidgets('editing', (tester) async {
        when(() => chatDetailsCubit.state).thenReturn(
          ChatDetailsState(
            chat: _chatWithDraft(
              UiMessageDraft(
                message: 'Corrected message text',
                editingId: 1.messageId(),
                updatedAt: DateTime.parse('2023-01-01T00:00:00.000Z'),
                isCommitted: true,
              ),
            ),
            members: members,
          ),
        );
        messageListCubit.setState([
          _msg(1, 'The user is editing one of their own messages.'),
          _msg(2, 'Cancel on the left, confirm on the right. No plus button.'),
        ]);

        await tester.pumpWidget(buildSubject());
        await tester.pump();
        // Enter text and flush the 1-second draft debounce timer.
        await tester.enterText(
          find.byType(TextField),
          'Corrected message text',
        );
        await tester.pump(const Duration(seconds: 1));

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/composer_editing.png'),
        );
      });

      // State 6: Quote (reply) — plus on the left, send arrow on the right.
      // A reply bubble is shown inside the input field.
      testWidgets('quote', (tester) async {
        final replyContent = UiMimiContent(
          plainBody: 'This is the original message being replied to.',
          topicId: Uint8List(0),
          content: simpleMessage(
            'This is the original message being replied to.',
          ),
          attachments: [],
        );
        when(() => chatDetailsCubit.state).thenReturn(
          ChatDetailsState(
            chat: _chatWithDraft(
              UiMessageDraft(
                message: 'Replying to the message above',
                inReplyTo: (
                  UiMimiId(field0: U8Array32(Uint8List(32))),
                  UiInReplyToMessage.resolved(
                    messageId: 1.messageId(),
                    sender: 2.userId(),
                    mimiContent: replyContent,
                  ),
                ),
                updatedAt: DateTime.parse('2023-01-01T00:00:00.000Z'),
                isCommitted: true,
              ),
            ),
            members: members,
          ),
        );
        messageListCubit.setState([
          _msg(1, 'The user is replying to another message.'),
          _msg(2, 'A reply bubble appears inside the input field.'),
        ]);

        await tester.pumpWidget(buildSubject());
        await tester.pump();
        // Enter text and flush the 1-second draft debounce timer.
        await tester.enterText(
          find.byType(TextField),
          'Replying to the message above',
        );
        await tester.pump(const Duration(seconds: 1));

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/composer_quote.png'),
        );
      });
    });

    testWidgets('renders correctly', (tester) async {
      messageListCubit.setState(messages);

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_screen.png'),
      );
    });

    testWidgets('renders correctly (dark mode)', (tester) async {
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(() {
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      messageListCubit.setState(messages);

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/chat_screen_dark.png'),
      );
    });
  });
}
