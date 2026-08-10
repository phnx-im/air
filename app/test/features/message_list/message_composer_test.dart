// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
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
  late MockNavigationCubit navigationCubit;
  late MockUserCubit userCubit;
  late MockUsersCubit usersCubit;
  late MockChatDetailsCubit chatDetailsCubit;
  late MockMessageListCubit messageListCubit;
  late MockUserSettingsCubit userSettingsCubit;
  late TextEditingController inputController;

  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
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
