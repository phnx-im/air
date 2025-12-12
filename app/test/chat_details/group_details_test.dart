// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details.dart';
import 'package:air/chat/group_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../mocks.dart';

void main() {
  group('GroupDetails', () {
    late MockChatDetailsCubit chatDetailsCubit;
    late MockUsersCubit usersCubit;
    late MockUserCubit userCubit;

    setUp(() async {
      chatDetailsCubit = MockChatDetailsCubit();
      usersCubit = MockUsersCubit();
      userCubit = MockUserCubit();

      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => userCubit.state,
      ).thenReturn(MockUiUser(id: 1, userHandles: const []));
    });

    Widget buildSubject({List<UiUserId> members = const []}) {
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chats[2], members: members));

      return MultiBlocProvider(
        providers: [
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
        ],
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: lightTheme,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const Scaffold(body: GroupDetails()),
        ),
      );
    }

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details.png'),
      );
    });

    testWidgets('renders correctly with members overflowing', (tester) async {
      final members = (userProfiles + userProfiles + userProfiles)
          .map((e) => e.userId)
          .toList();
      await tester.pumpWidget(buildSubject(members: members));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details_members_overflow.png'),
      );
    });

    testWidgets('renders correctly empty', (tester) async {
      await tester.pumpWidget(buildSubject(members: []));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/group_details_empty.png'),
      );
    });
  });
}
