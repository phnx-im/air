// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/core.dart';
import 'package:air/features/developer/change_user_modal.dart';
import 'package:air/features/user/user_session_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:provider/provider.dart';
import 'package:uuid/uuid.dart';

import '../../helpers.dart';
import '../../mocks.dart';

final clientRecords = [
  UiClientRecord(
    clientRecordId: 1.clientRecordId(),
    userId: 1.userId(),
    createdAt: DateTime.parse("2023-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 1.userId(), displayName: "alice"),
    isFinished: true,
  ),
  UiClientRecord(
    clientRecordId: 2.clientRecordId(),
    userId: 2.userId(),
    createdAt: DateTime.parse("2024-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 2.userId(), displayName: "alice"),
    isFinished: true,
  ),
  UiClientRecord(
    clientRecordId: 3.clientRecordId(),
    userId: 3.userId(),
    createdAt: DateTime.parse("2025-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 3.userId(), displayName: "bob"),
    isFinished: false,
  ),
];

void main() {
  setUpAll(() {
    registerFallbackValue(0.clientRecordId());
  });

  group('ChangeUserModal', () {
    late MockUser user;
    late MockUserSessionCubit userSessionCubit;
    late MockUsersCubit usersCubit;
    late MockCoreClient coreClient;
    late List<UiClientRecord> storedRecords;

    setUp(() async {
      user = MockUser();
      usersCubit = MockUsersCubit();
      userSessionCubit = MockUserSessionCubit();
      coreClient = MockCoreClient();
      storedRecords = List.of(clientRecords);

      when(() => user.userId).thenReturn(1.userId());
      when(() => user.clientRecordId).thenReturn(1.clientRecordId());
      when(() => userSessionCubit.state)
          .thenReturn(UserSessionState(user: user));
      when(() => usersCubit.state).thenReturn(
        MockUsersState(
          defaultUserId: 1.userId(),
          profiles: clientRecords.map((record) => record.userProfile).toList(),
        ),
      );
      when(() => coreClient.deleteUnloadedDatabase(any()))
          .thenAnswer((invocation) async {
            final id = invocation.positionalArguments.single as UuidValue;
            storedRecords.removeWhere((record) => record.clientRecordId == id);
          });
      scaffoldMessengerKey.currentState?.clearSnackBars();
    });

    Widget buildSubject({bool scaffold = false}) => MultiBlocProvider(
      providers: [
        Provider<CoreClient>.value(value: coreClient),
        BlocProvider<UserSessionCubit>.value(value: userSessionCubit),
        BlocProvider<UsersCubit>.value(value: usersCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            scaffoldMessengerKey: scaffoldMessengerKey,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            builder: scaffold
                ? (context, child) => RootScaffold(child: child!)
                : null,
            home: ChangeUserModal(
              loadClientRecords: () async => List.of(storedRecords),
            ),
          );
        },
      ),
    );

    final deleteButtons = find.byTooltip('Delete account');

    Finder deleteButtonOf(String displayName) => find.descendant(
      of: find.widgetWithText(Row, displayName),
      matching: deleteButtons,
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/change_user_modal.png'),
      );
    });

    testWidgets('offers no delete for the loaded account', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(deleteButtons, findsNWidgets(2));
      expect(deleteButtonOf('alice'), findsOneWidget);
      expect(deleteButtonOf('bob'), findsOneWidget);
    });

    testWidgets('deletes an account after confirmation', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await tester.tap(deleteButtonOf('bob'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Delete'));
      await tester.pumpAndSettle();

      verify(() => coreClient.deleteUnloadedDatabase(3.clientRecordId()))
          .called(1);
      verifyNever(() => coreClient.logout());
      expect(find.text('bob'), findsNothing);
      expect(find.text('alice'), findsOneWidget);
    });

    testWidgets('keeps the account when cancelled', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await tester.tap(deleteButtonOf('bob'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();

      verifyNever(() => coreClient.deleteUnloadedDatabase(any()));
      expect(find.text('bob'), findsOneWidget);
    });

    testWidgets('reports a failed deletion and reloads the list', (
      tester,
    ) async {
      when(() => coreClient.deleteUnloadedDatabase(any()))
          .thenAnswer((_) async {
            storedRecords.removeWhere(
              (record) => record.clientRecordId == 3.clientRecordId(),
            );
            throw Exception('disk full');
          });

      await tester.pumpWidget(buildSubject(scaffold: true));
      await tester.pumpAndSettle();

      await tester.tap(deleteButtonOf('bob'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Delete'));
      await tester.pumpAndSettle();

      expect(
        find.text('Failed to delete account: Exception: disk full'),
        findsOneWidget,
      );
      expect(find.text('bob'), findsNothing);
    });
  });
}
