// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/app.dart' show scaffoldMessengerKey;
import 'package:air/core/core.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';
import '../chat_list/chat_list_content_test.dart';
import 'message_list_test.dart';

final _chat = chats[2]; // Group chat, isConfirmed = true

final _members = [1.userId(), 2.userId(), 3.userId()];

final _navState = NavigationState.home(
  home: HomeNavigationState(chatId: _chat.id),
);

/// The composer's draft debounce.
const _draftDebounce = Duration(seconds: 1);

void main() {
  late AppLocalizations loc;

  late MockNavigationCubit navigationCubit;
  late MockUserCubit userCubit;
  late MockUsersCubit usersCubit;
  late MockChatDetailsCubit chatDetailsCubit;
  late MockMessageListCubit messageListCubit;
  late MockUserSettingsCubit userSettingsCubit;
  late TextEditingController inputController;

  setUpAll(() async {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
    loc = await AppLocalizations.delegate.load(const Locale('en'));
  });

  setUp(() {
    navigationCubit = MockNavigationCubit();
    userCubit = MockUserCubit();
    usersCubit = MockUsersCubit();
    chatDetailsCubit = MockChatDetailsCubit();
    messageListCubit = MockMessageListCubit();
    userSettingsCubit = MockUserSettingsCubit();
    // The composer owns the controller it is handed and disposes it, so the
    // test must not dispose it a second time.
    inputController = TextEditingController();

    when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
    when(
      () => usersCubit.state,
    ).thenReturn(MockUsersState(profiles: userProfiles));
    when(
      () => chatDetailsCubit.state,
    ).thenReturn(ChatDetailsState(chat: _chat, members: _members));
    when(
      () => chatDetailsCubit.markAsRead(
        untilMessageId: any(named: "untilMessageId"),
        untilTimestamp: any(named: "untilTimestamp"),
      ),
    ).thenAnswer((_) async {});
    when(
      () => chatDetailsCubit.storeDraft(
        draftMessage: any(named: "draftMessage"),
        isCommitted: any(named: "isCommitted"),
      ),
    ).thenAnswer((_) async {});
    when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    when(() => navigationCubit.state).thenReturn(_navState);

    messageListCubit.setState(messages);
  });

  // The snackbar goes through the global scaffold messenger, so the app under
  // test has to carry the very key `showSnackBarStandalone` reaches for.
  Widget buildSubject() => MultiBlocProvider(
    providers: [
      BlocProvider<NavigationCubit>.value(value: navigationCubit),
      BlocProvider<UserCubit>.value(value: userCubit),
      BlocProvider<UsersCubit>.value(value: usersCubit),
      BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
      BlocProvider<MessageListCubit>.value(value: messageListCubit),
      BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
    ],
    child: Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        scaffoldMessengerKey: scaffoldMessengerKey,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Scaffold(
          body: ChatScreenView(
            createMessageCubit: createMockMessageCubit,
            textEditingController: inputController,
          ),
        ),
      ),
    ),
  );

  group('MessageComposer send failure', () {
    final sendButton = find.byWidgetPredicate(
      (widget) => widget is ButtonIcon && widget.icon == AppIconType.arrowUp,
    );

    Finder errorSnackbar() => find.text(loc.composer_error_sendMessage);

    Future<void> typeAndSend(WidgetTester tester, String text) async {
      await tester.enterText(find.byType(TextField), text);
      await tester.pumpAndSettle();
      await tester.tap(sendButton);
      await tester.pumpAndSettle();
    }

    testWidgets('restores the text and shows an error snackbar', (
      tester,
    ) async {
      when(
        () => chatDetailsCubit.sendMessage(any()),
      ).thenAnswer((_) async => throw Exception('send failed'));

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await typeAndSend(tester, 'Hello there');

      expect(errorSnackbar(), findsOneWidget);
      expect(inputController.text, 'Hello there');
    });

    testWidgets('keeps text typed while the send was in flight', (
      tester,
    ) async {
      final sent = Completer<void>();
      when(
        () => chatDetailsCubit.sendMessage(any()),
      ).thenAnswer((_) => sent.future);

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await typeAndSend(tester, 'Hello there');
      expect(inputController.text, isEmpty);

      await tester.enterText(find.byType(TextField), 'Second thoughts');
      await tester.pumpAndSettle();

      sent.completeError(Exception('send failed'));
      await tester.pumpAndSettle();

      expect(errorSnackbar(), findsOneWidget);
      expect(inputController.text, 'Second thoughts');
    });

    testWidgets('leaves the input empty when the send succeeds', (
      tester,
    ) async {
      when(() => chatDetailsCubit.sendMessage(any())).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await typeAndSend(tester, 'Hello there');

      expect(errorSnackbar(), findsNothing);
      expect(inputController.text, isEmpty);
      verify(() => chatDetailsCubit.sendMessage('Hello there')).called(1);
    });

    testWidgets('does not store the emptied input as a draft', (tester) async {
      when(() => chatDetailsCubit.sendMessage(any())).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await typeAndSend(tester, 'Hello there');
      // Past the debounce the cleared input would have been stored, which is
      // the write that races the send.
      await tester.pump(_draftDebounce);

      verifyNever(
        () => chatDetailsCubit.storeDraft(draftMessage: '', isCommitted: false),
      );
    });
  });

  group('MessageComposer disposal', () {
    testWidgets('drops the pending draft store instead of un-committing', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await tester.enterText(find.byType(TextField), 'Hello there');
      await tester.pumpAndSettle();

      // Tear the tree down while the keystroke's store is still debounced.
      await tester.pumpWidget(const SizedBox());
      await tester.pump(_draftDebounce);

      verify(
        () => chatDetailsCubit.storeDraft(
          draftMessage: 'Hello there',
          isCommitted: true,
        ),
      ).called(1);
      verifyNever(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: false,
        ),
      );
    });
  });
}
