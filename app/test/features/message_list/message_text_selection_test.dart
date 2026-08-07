// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/patterns/reaction_bar/reaction_bar.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/message_list/message_cubit.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/message_list/message_list_view.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/gestures.dart' show kDoubleTapMinTime;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../../mocks.dart';

const Size _testSize = Size(1080, 2800);

const String _messageText = 'Hello Alice';

final _chatId = 1.chatId();

final _messages = [
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
          plainBody: _messageText,
          topicId: Uint8List(0),
          content: simpleMessage(_messageText),
          attachments: [],
        ),
      ),
    ),
    status: UiMessageStatus.sent,
    reactions: [],
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

  group('Message text selection on touch', () {
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockAttachmentsRepository attachmentsRepository;
    late MockUserSettingsCubit userSettingsCubit;
    late List<String> copied;

    setUp(() {
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      attachmentsRepository = MockAttachmentsRepository();
      userSettingsCubit = MockUserSettingsCubit();
      copied = [];

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
      when(
        () =>
            chatDetailsCubit.replyToMessage(messageId: any(named: 'messageId')),
      ).thenAnswer((_) async {});
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      messageListCubit.setState(_messages);

      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(SystemChannels.platform, (call) async {
            if (call.method == 'Clipboard.setData') {
              copied.add((call.arguments as Map)['text'] as String);
            }
            return null;
          });
    });

    tearDown(() {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(SystemChannels.platform, null);
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
            final theme = testThemeData(
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

    Future<void> pumpList(WidgetTester tester) async {
      tester.view.physicalSize = _testSize;
      addTearDown(tester.view.resetPhysicalSize);
      await tester.pumpWidget(buildSubject());
      await tester.pump();
    }

    /// Taps [count] times in a row on [position], close enough in time to form
    /// one series.
    Future<void> tapSeries(
      WidgetTester tester,
      Offset position, {
      required int count,
    }) async {
      for (var i = 0; i < count; i++) {
        if (i > 0) await tester.pump(kDoubleTapMinTime);
        await tester.tapAt(position);
      }
      await tester.pumpAndSettle();
    }

    /// What the selection holds, taken through the toolbar's own copy.
    Future<String?> copySelection(WidgetTester tester) async {
      final copy = find.descendant(
        of: find.byType(AdaptiveTextSelectionToolbar),
        matching: find.text('Copy'),
      );
      if (copy.evaluate().isEmpty) return null;
      await tester.tap(copy);
      await tester.pumpAndSettle();
      return copied.isEmpty ? null : copied.last;
    }

    /// Left of the first word, past the bubble's own padding.
    Offset firstWord(WidgetTester tester) {
      final text = tester.getRect(find.text(_messageText));
      return Offset(text.left + 4, text.center.dy);
    }

    testWidgets('double tap selects the word under the finger', (tester) async {
      await pumpList(tester);

      await tapSeries(tester, firstWord(tester), count: 2);

      expect(await copySelection(tester), 'Hello');
      // The quick reactions used to sit on the double tap, where they took the
      // gesture away from the word selection.
      expect(find.byType(ReactionBar), findsNothing);
    });

    testWidgets('triple tap selects the whole message', (tester) async {
      await pumpList(tester);

      await tapSeries(tester, firstWord(tester), count: 3);

      expect(await copySelection(tester), _messageText);
    });

    testWidgets('long press opens the message actions, not a selection', (
      tester,
    ) async {
      await pumpList(tester);

      await tester.longPressAt(firstWord(tester));
      await tester.pumpAndSettle();

      final loc = await AppLocalizations.delegate.load(const Locale('en'));
      expect(find.text(loc.messageContextMenu_reply), findsOneWidget);
      expect(find.byType(AdaptiveTextSelectionToolbar), findsNothing);
    });

    testWidgets('a swipe on the text still replies', (tester) async {
      await pumpList(tester);

      await tester.drag(find.text(_messageText), const Offset(80, 0));
      await tester.pumpAndSettle();

      verify(
        () => chatDetailsCubit.replyToMessage(messageId: 1.messageId()),
      ).called(1);
    });
  });
}
