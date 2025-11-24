// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/connection_details.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';

import '../chat_list/chat_list_content_test.dart';
import '../mocks.dart';

final chat = chats[0];

void main() {
  group('ConnectionDetailsTest', () {
    late MockChatDetailsCubit chatDetailsCubit;
    late MockUsersCubit usersCubit;

    setUp(() async {
      chatDetailsCubit = MockChatDetailsCubit();
      usersCubit = MockUsersCubit();

      when(() => chatDetailsCubit.state).thenReturn(
        ChatDetailsState(chat: chat, members: [userProfiles[1].userId]),
      );
      when(() => usersCubit.state).thenReturn(
        MockUsersState(profiles: userProfiles),
      );
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
        BlocProvider<UsersCubit>.value(value: usersCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: lightTheme,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const Scaffold(body: ConnectionDetails()),
          );
        },
      ),
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/connection_details.png'),
      );
    });
  });
}
