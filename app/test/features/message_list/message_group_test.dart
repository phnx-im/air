// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/core/core.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:air/ds/patterns/message_row/message_row.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/user/avatar.dart';
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

/// A content message from [sender], [after] the start of the fixture clock.
UiChatMessage _message(
  int id, {
  required int sender,
  Duration after = Duration.zero,
  String? text,
  List<UiReaction> reactions = const [],
}) => UiChatMessage(
  id: id.messageId(),
  chatId: _chatId,
  timestamp: _start.add(after),
  message: UiMessage_Content(
    UiContentMessage(
      sender: sender.userId(),
      sent: true,
      edited: false,
      content: UiMimiContent(
        plainBody: text ?? 'Message $id',
        topicId: Uint8List(0),
        content: simpleMessage(text ?? 'Message $id'),
        attachments: [],
      ),
    ),
  ),
  status: UiMessageStatus.read,
  reactions: reactions,
);

/// A non-content message, which never joins a group.
UiChatMessage _event(int id, {Duration after = Duration.zero}) => UiChatMessage(
  id: id.messageId(),
  chatId: _chatId,
  timestamp: _start.add(after),
  message: const UiMessage_Display(
    UiEventMessage.error(UiErrorMessage(message: 'Something went wrong')),
  ),
  status: UiMessageStatus.read,
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

  group('message groups', () {
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

    /// The group boundaries the list derived for the row of [id], as
    /// `(starts, ends)`.
    (bool, bool) boundariesOf(WidgetTester tester, int id) {
      final tile = tester.widget<TextMessageTile>(rowOf(id));
      return (tile.startsMessageGroup, tile.endsMessageGroup);
    }

    testWidgets('join messages from the same sender within five minutes', (
      tester,
    ) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _message(2, sender: 2, after: const Duration(minutes: 1)),
        _message(3, sender: 2, after: const Duration(minutes: 2)),
      ]);

      await tester.pumpWidget(buildSubject());

      expect(boundariesOf(tester, 1), (true, false));
      expect(boundariesOf(tester, 2), (false, false));
      expect(boundariesOf(tester, 3), (false, true));
    });

    testWidgets('break where the sender changes', (tester) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _message(2, sender: 3, after: const Duration(minutes: 1)),
        _message(3, sender: 2, after: const Duration(minutes: 2)),
      ]);

      await tester.pumpWidget(buildSubject());

      expect(boundariesOf(tester, 1), (true, true));
      expect(boundariesOf(tester, 2), (true, true));
      expect(boundariesOf(tester, 3), (true, true));
    });

    testWidgets('hold together just under five minutes', (tester) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _message(2, sender: 2, after: const Duration(minutes: 4, seconds: 59)),
      ]);

      await tester.pumpWidget(buildSubject());

      expect(boundariesOf(tester, 1), (true, false));
      expect(boundariesOf(tester, 2), (false, true));
    });

    testWidgets('break at exactly five minutes', (tester) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _message(2, sender: 2, after: const Duration(minutes: 5)),
      ]);

      await tester.pumpWidget(buildSubject());

      expect(boundariesOf(tester, 1), (true, true));
      expect(boundariesOf(tester, 2), (true, true));
    });

    testWidgets('break beyond five minutes', (tester) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _message(2, sender: 2, after: const Duration(minutes: 30)),
      ]);

      await tester.pumpWidget(buildSubject());

      expect(boundariesOf(tester, 1), (true, true));
      expect(boundariesOf(tester, 2), (true, true));
    });

    testWidgets('break around a non-content message', (tester) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _event(2, after: const Duration(minutes: 1)),
        _message(3, sender: 2, after: const Duration(minutes: 2)),
      ]);

      await tester.pumpWidget(buildSubject());

      expect(boundariesOf(tester, 1), (true, true));
      expect(boundariesOf(tester, 3), (true, true));
    });

    testWidgets('break at the unread divider', (tester) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _message(2, sender: 2, after: const Duration(minutes: 1)),
        _message(3, sender: 2, after: const Duration(minutes: 2)),
        _message(4, sender: 2, after: const Duration(minutes: 3)),
      ], firstUnreadIndex: 2);

      await tester.pumpWidget(buildSubject());

      // Without the divider all four would be one group.
      expect(boundariesOf(tester, 1), (true, false));
      expect(boundariesOf(tester, 2), (false, true));
      expect(boundariesOf(tester, 3), (true, false));
      expect(boundariesOf(tester, 4), (false, true));
    });

    testWidgets('close at the edges of the loaded window', (tester) async {
      messageListCubit.setState(
        [
          _message(1, sender: 2),
          _message(2, sender: 2, after: const Duration(minutes: 1)),
        ],
        hasOlder: true,
        hasNewer: true,
      );

      await tester.pumpWidget(buildSubject());

      // The rows that would join them are not loaded, so the window edges
      // stand in for a sender change.
      expect(boundariesOf(tester, 1), (true, false));
      expect(boundariesOf(tester, 2), (false, true));
    });

    testWidgets(
      'name the sender on the first row of a group and show the avatar on the '
      'last',
      (tester) async {
        messageListCubit.setState([
          _message(1, sender: 2),
          _message(2, sender: 2, after: const Duration(minutes: 1)),
          _message(3, sender: 2, after: const Duration(minutes: 2)),
        ]);

        await tester.pumpWidget(buildSubject());

        MessageRow rowFor(int id) => tester.widget<MessageRow>(
          find.descendant(of: rowOf(id), matching: find.byType(MessageRow)),
        );

        expect(rowFor(1).senderName, 'Bob');
        expect(rowFor(2).senderName, isNull);
        expect(rowFor(3).senderName, isNull);

        expect(rowFor(1).avatar, isNull);
        expect(rowFor(2).avatar, isNull);
        expect(rowFor(3).avatar, isNotNull);

        expect(find.text('Bob'), findsOneWidget);
      },
    );

    testWidgets('rest the avatar on the bottom edge of the last bubble', (
      tester,
    ) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _message(2, sender: 2, after: const Duration(minutes: 1)),
        // A newer group, so the row above ends its group without carrying
        // the newest-message stamp.
        _message(3, sender: 3, after: const Duration(minutes: 2)),
      ]);

      await tester.pumpWidget(buildSubject());

      Rect rectOf(int id, Type type) => tester.getRect(
        find.descendant(of: rowOf(id), matching: find.byType(type)),
      );

      // Row 2 carries nothing below the bubble, row 3 carries the newest
      // message's stamp.
      expect(
        rectOf(2, UserAvatar).bottom,
        moreOrLessEquals(rectOf(2, MessageBubble).bottom),
      );
      expect(
        rectOf(3, UserAvatar).bottom,
        moreOrLessEquals(rectOf(3, MessageBubble).bottom),
      );
    });

    testWidgets('rest the avatar on the bubble above the reaction chips', (
      tester,
    ) async {
      const long =
          'A message long enough that its bubble runs most of the way across '
          'the conversation, which is what leaves the chips the stamp room.';
      messageListCubit.setState([
        // The stamp shares the chips' line on the wide bubble and drops below
        // the run on the narrow one, and the avatar has to clear both.
        _message(
          1,
          sender: 2,
          text: 'Hi',
          reactions: [
            UiReaction(emoji: '👍', users: [1.userId(), 3.userId()]),
            UiReaction(emoji: '🎉', users: [1.userId()]),
            UiReaction(emoji: '💖', users: [3.userId()]),
          ],
        ),
        _message(
          2,
          sender: 3,
          after: const Duration(minutes: 1),
          text: long,
          reactions: [
            UiReaction(emoji: '👍', users: [1.userId()]),
          ],
        ),
      ]);

      await tester.pumpWidget(buildSubject());

      Rect rectOf(int id, Type type) => tester.getRect(
        find.descendant(of: rowOf(id), matching: find.byType(type)),
      );

      expect(
        rectOf(1, UserAvatar).bottom,
        moreOrLessEquals(rectOf(1, MessageBubble).bottom),
      );
      expect(
        rectOf(2, UserAvatar).bottom,
        moreOrLessEquals(rectOf(2, MessageBubble).bottom),
      );
    });

    testWidgets('sit closer together inside a group than between groups', (
      tester,
    ) async {
      messageListCubit.setState([
        _message(1, sender: 2),
        _message(2, sender: 2, after: const Duration(minutes: 1)),
        // A new group: the sender changed.
        _message(3, sender: 3, after: const Duration(minutes: 2)),
      ]);

      await tester.pumpWidget(buildSubject());

      // Older messages sit above newer ones, so a row's gap is the distance
      // from the bottom of the row before it.
      double gapAbove(int id, {required int below}) =>
          tester.getRect(rowOf(id)).top - tester.getRect(rowOf(below)).bottom;

      final withinGroup = gapAbove(2, below: 1);
      final betweenGroups = gapAbove(3, below: 2);
      expect(withinGroup, lessThan(betweenGroups));
    });
  });
}
