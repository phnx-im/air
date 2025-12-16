// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/message_list/contact_request_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';

import '../helpers.dart';
import '../mocks.dart';
import '../chat_list/chat_list_content_test.dart';

void main() {
  group('ContactRequestDialog', () {
    late MockUsersCubit usersCubit;

    setUp(() async {
      usersCubit = MockUsersCubit();

      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
    });

    Widget buildSubject({
      required UiUserId sender,
      required ContactRequestSource source,
    }) => MultiBlocProvider(
      providers: [BlocProvider<UsersCubit>.value(value: usersCubit)],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: themeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: Scaffold(
              body: ContactRequestDialog(sender: sender, source: source),
            ),
          );
        },
      ),
    );

    testWidgets('renders correctly for handle', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          sender: 1.userId(),
          source: const .handle(
            handle: UiUserHandle(plaintext: "some-user-name"),
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_request_dialog_handle.png'),
      );
    });

    testWidgets('renders correctly for targeted message', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          sender: 1.userId(),
          source: const .targetedMessage(originChatTitle: "some-chat-name"),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile(
          'goldens/contact_request_dialog_targeted_message.png',
        ),
      );
    });
  });
}
