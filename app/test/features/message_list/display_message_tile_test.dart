// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/message_list/display_message_tile.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';
import '../chat_list/chat_list_content_test.dart';

/// The reader, in every test here. Eve is neither of the two people the
/// sentences name, so a name tapped in them is somebody else's.
final eve = 3.userId();

void main() {
  group('DisplayMessageTile', () {
    late MockUsersCubit usersCubit;
    late MockUserCubit userCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockNavigationCubit navigationCubit;

    setUpAll(() {
      registerFallbackValue(eve);
    });

    setUp(() {
      usersCubit = MockUsersCubit();
      userCubit = MockUserCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      navigationCubit = MockNavigationCubit();

      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(() => userCubit.state).thenReturn(MockUiUser(id: 3));
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(const ChatDetailsState(members: []));
    });

    Widget buildSubject(UiSystemMessage message) => MultiBlocProvider(
      providers: [
        BlocProvider<UsersCubit>.value(value: usersCubit),
        BlocProvider<UserCubit>.value(value: userCubit),
        BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
      ],
      child: Builder(
        builder: (context) => MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: Scaffold(
            body: DisplayMessageTile(
              UiEventMessage.system(message),
              DateTime.utc(2026, 1, 1),
            ),
          ),
        ),
      ),
    );

    testWidgets('opens the profile of the name that was tapped', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(UiSystemMessage.add(1.userId(), 2.userId())),
      );

      await tester.tapOnText(find.textRange.ofSubstring('Bob'));

      verify(() => navigationCubit.openMemberDetails(2.userId())).called(1);
    });

    testWidgets('opens nothing from the words between the names', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(UiSystemMessage.add(1.userId(), 2.userId())),
      );

      await tester.tapOnText(find.textRange.ofSubstring('added'));

      verifyNever(() => navigationCubit.openMemberDetails(any()));
    });

    testWidgets('opens nothing from the reader\'s own name', (tester) async {
      await tester.pumpWidget(
        buildSubject(UiSystemMessage.add(eve, 2.userId())),
      );

      await tester.tapOnText(find.textRange.ofSubstring('Eve'));

      verifyNever(() => navigationCubit.openMemberDetails(any()));
    });

    testWidgets('opens nothing from a group name', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          UiSystemMessage.changeTitle(1.userId(), 'Weekend', 'Weekly sync'),
        ),
      );

      await tester.tapOnText(find.textRange.ofSubstring('Weekly sync'));

      verifyNever(() => navigationCubit.openMemberDetails(any()));
    });
  });
}
