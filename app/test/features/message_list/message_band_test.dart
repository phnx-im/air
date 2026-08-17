// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/core/core.dart';
import 'package:air/ds/components/reaction_chip/reaction_chip.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:air/ds/patterns/message_meta/message_meta.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/message_list/message_band.dart';
import 'package:air/features/message_list/message_cubit.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/message_list/message_list_view.dart';
import 'package:air/features/message_list/text_message_tile.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../../mocks.dart';

final _chatId = 1.chatId();
final _start = DateTime.parse('2023-01-01T12:00:00.000Z');

const _long =
    'A message long enough that its bubble runs most of the way across '
    'the conversation, which is what leaves a run of chips room to spare.';

UiChatMessage _message(
  int id, {
  required int sender,
  required String text,
  List<UiReaction> reactions = const [],
}) => UiChatMessage(
  id: id.messageId(),
  chatId: _chatId,
  timestamp: _start.add(Duration(minutes: id * 6)),
  message: UiMessage_Content(
    UiContentMessage(
      sender: sender.userId(),
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
  status: UiMessageStatus.read,
  reactions: reactions,
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

  group('message band', () {
    late MockUserCubit userCubit;
    late MockUsersCubit usersCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockAttachmentsRepository attachmentsRepository;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() {
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      attachmentsRepository = MockAttachmentsRepository();
      userSettingsCubit = MockUserSettingsCubit();

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: 'untilMessageId'),
          untilTimestamp: any(named: 'untilTimestamp'),
        ),
      ).thenAnswer((_) async {});
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    });

    Widget buildSubject() => RepositoryProvider<AttachmentsRepository>.value(
      value: attachmentsRepository,
      child: MultiBlocProvider(
        providers: [
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<MessageListCubit>.value(value: messageListCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: Builder(
          builder: (context) => MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const Scaffold(
              body: MessageListView(
                createMessageCubit: _createMockMessageCubit,
              ),
            ),
          ),
        ),
      ),
    );

    Finder rowOf(int id) => find.byWidgetPredicate(
      (widget) =>
          widget is TextMessageTile && widget.messageId == id.messageId(),
    );

    Finder inRow(int id, Type type) =>
        find.descendant(of: rowOf(id), matching: find.byType(type));

    /// The bubble and the stamp of the row of [id]. The newest row always
    /// carries a stamp, which is the row these tests measure.
    (Rect bubble, Rect stamp) boxesOf(WidgetTester tester, int id) => (
      tester.getRect(inRow(id, MessageBubble)),
      tester.getRect(inRow(id, MessageMeta)),
    );

    testWidgets('hangs the stamp off the bottom of an unreacted bubble', (
      tester,
    ) async {
      messageListCubit.setState([_message(1, sender: 1, text: _long)]);

      await tester.pumpWidget(buildSubject());

      final (bubble, stamp) = boxesOf(tester, 1);
      expect(stamp.top, bubble.bottom);
      expect(stamp.right, bubble.right);
    });

    testWidgets('keeps that place where the chips leave the stamp room', (
      tester,
    ) async {
      messageListCubit.setState([
        _message(
          1,
          sender: 1,
          text: _long,
          reactions: [
            UiReaction(emoji: '👍', users: [2.userId()]),
          ],
        ),
      ]);

      await tester.pumpWidget(buildSubject());

      final (bubble, stamp) = boxesOf(tester, 1);
      expect(stamp.top, bubble.bottom);
      expect(stamp.right, bubble.right);
      // The two share the line, so they have to clear each other on it.
      expect(
        tester.getRect(inRow(1, ReactionChip)).right,
        lessThan(stamp.left),
      );
    });

    testWidgets('drops the stamp below a run that fills the bubble', (
      tester,
    ) async {
      messageListCubit.setState([
        _message(
          1,
          sender: 1,
          text: 'Hi',
          reactions: [
            UiReaction(emoji: '👍', users: [2.userId(), 3.userId()]),
            UiReaction(emoji: '🎉', users: [2.userId()]),
            UiReaction(emoji: '💖', users: [3.userId()]),
          ],
        ),
      ]);

      await tester.pumpWidget(buildSubject());

      final (bubble, stamp) = boxesOf(tester, 1);
      expect(stamp.top, greaterThan(bubble.bottom));
    });

    // The chips take an incoming row's leading edge, so the stamp moves to
    // the trailing one and the two share the line like on an own message.
    testWidgets('shares the line on an incoming message with reactions', (
      tester,
    ) async {
      messageListCubit.setState([
        _message(
          1,
          sender: 2,
          text: _long,
          reactions: [
            UiReaction(emoji: '👍', users: [1.userId()]),
          ],
        ),
      ]);

      await tester.pumpWidget(buildSubject());

      final (bubble, stamp) = boxesOf(tester, 1);
      expect(stamp.top, bubble.bottom);
      expect(stamp.right, bubble.right);
      expect(
        tester.getRect(inRow(1, ReactionChip)).right,
        lessThan(stamp.left),
      );
    });

    testWidgets('drops an incoming stamp below a run that fills the bubble', (
      tester,
    ) async {
      messageListCubit.setState([
        _message(
          1,
          sender: 2,
          text: 'Hi',
          reactions: [
            UiReaction(emoji: '👍', users: [2.userId(), 3.userId()]),
            UiReaction(emoji: '🎉', users: [2.userId()]),
            UiReaction(emoji: '💖', users: [3.userId()]),
          ],
        ),
      ]);

      await tester.pumpWidget(buildSubject());

      final (bubble, stamp) = boxesOf(tester, 1);
      expect(stamp.top, greaterThan(bubble.bottom));
    });

    testWidgets('keeps an unreacted incoming stamp on the leading edge', (
      tester,
    ) async {
      messageListCubit.setState([_message(1, sender: 2, text: _long)]);

      await tester.pumpWidget(buildSubject());

      final (bubble, stamp) = boxesOf(tester, 1);
      expect(stamp.top, bubble.bottom);
      expect(stamp.left, bubble.left);
    });
  });

  group('message band reveal', () {
    const bubbleKey = Key('bubble');
    const stampKey = Key('stamp');

    Widget host(List<UiReaction> reactions) => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testThemeData(Brightness.light),
      home: Scaffold(
        body: Align(
          alignment: Alignment.topRight,
          child: MessageBand(
            outgoing: true,
            bubble: const SizedBox(key: bubbleKey, width: 300, height: 40),
            stamp: const SizedBox(key: stampKey, width: 80, height: 18),
            affordance: null,
            reactions: reactions,
            ownUserId: 1.userId(),
            onTapReaction: (_) {},
          ),
        ),
      ),
    );

    // The reserve opens under the bubble as the chips arrive. A stamp that
    // shares their line sits above it and has nowhere to go, so it must hold
    // still for the whole reveal rather than riding the reserve down and back.
    testWidgets('leaves a shared stamp where it stands', (tester) async {
      await tester.pumpWidget(host(const []));
      final resting = tester.getRect(find.byKey(stampKey)).top;

      await tester.pumpWidget(
        host([
          UiReaction(emoji: '👍', users: [2.userId()]),
        ]),
      );
      await tester.pump(const Duration(milliseconds: 40));

      expect(tester.hasRunningAnimations, isTrue);
      expect(tester.getRect(find.byKey(stampKey)).top, resting);

      await tester.pumpAndSettle();

      expect(tester.getRect(find.byKey(stampKey)).top, resting);
      expect(find.byType(ReactionChip), findsOneWidget);
    });
  });
}
