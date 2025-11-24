// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/chat/member_details_screen.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/user/user.dart';

import '../chat_list/chat_list_content_test.dart';
import '../mocks.dart';

void main() {
  group('MemberDetails', () {
    late MockUsersCubit usersCubit;

    setUp(() {
      usersCubit = MockUsersCubit();
      when(() => usersCubit.state).thenReturn(
        MockUsersState(profiles: userProfiles),
      );
    });

    Widget buildSubject() => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: lightTheme,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      home: MultiBlocProvider(
        providers: [
          BlocProvider<UsersCubit>.value(value: usersCubit),
        ],
        child: Scaffold(
          body: MemberDetailsView(
            chatId: chats[0].id,
            profile: userProfiles[1],
            isSelf: false,
            canKick: true,
          ),
        ),
      ),
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/members_details.png'),
      );
    });
  });
}
