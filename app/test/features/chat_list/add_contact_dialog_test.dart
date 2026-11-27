// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/chat_list/add_contact_dialog.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

class _FakeUsernameHash extends Fake implements UsernameHash {}

void main() {
  group('AddContactDialog opened with a username', () {
    const alice = UiUsername(plaintext: 'alice');
    final loc = lookupAppLocalizations(const Locale('en'));

    late MockUserCubit userCubit;

    setUpAll(() => registerFallbackValue(alice));

    setUp(() {
      userCubit = MockUserCubit();
      when(() => userCubit.state).thenReturn(
        MockUiUser(id: 1, usernames: const [UiUsername(plaintext: 'ellie')]),
      );
    });

    Future<void> pumpDialog(WidgetTester tester, {UiUsername? username}) =>
        tester.pumpWidget(
          BlocProvider<UserCubit>.value(
            value: userCubit,
            child: MaterialApp(
              theme: testLightTheme,
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(body: AddContactDialog(username: username)),
            ),
          ),
        );

    testWidgets('looks the username up and offers to send', (tester) async {
      when(() => userCubit.checkUsernameExists(username: alice))
          .thenAnswer((_) async => _FakeUsernameHash());

      await pumpDialog(tester, username: alice);
      await tester.pumpAndSettle();

      verify(() => userCubit.checkUsernameExists(username: alice)).called(1);
      final field = tester.widget<EditableText>(find.byType(EditableText));
      expect(field.controller.text, 'alice');
      expect(
        find.text(loc.newConnectionDialog_handleExists('alice')),
        findsOneWidget,
      );
      expect(find.text(loc.newConnectionDialog_confirm2), findsOneWidget);
    });

    testWidgets('shows that the username was not found', (tester) async {
      when(() => userCubit.checkUsernameExists(username: alice))
          .thenAnswer((_) async => null);

      await pumpDialog(tester, username: alice);
      await tester.pumpAndSettle();

      expect(
        find.text(loc.newConnectionDialog_error_usernameNotFound('alice')),
        findsOneWidget,
      );
      expect(find.text(loc.newConnectionDialog_confirm1), findsOneWidget);
    });

    testWidgets('rejects an own username without a lookup', (tester) async {
      when(() => userCubit.state)
          .thenReturn(MockUiUser(id: 1, usernames: const [alice]));

      await pumpDialog(tester, username: alice);
      await tester.pumpAndSettle();

      verifyNever(
        () => userCubit.checkUsernameExists(username: any(named: 'username')),
      );
      expect(
        find.text(loc.newConnectionDialog_error_ownUsername),
        findsOneWidget,
      );
    });

    testWidgets('looks nothing up without a username', (tester) async {
      await pumpDialog(tester);
      await tester.pumpAndSettle();

      verifyNever(
        () => userCubit.checkUsernameExists(username: any(named: 'username')),
      );
      expect(
        find.text(loc.newConnectionDialog_newConnectionDescription),
        findsOneWidget,
      );
    });
  });
}
