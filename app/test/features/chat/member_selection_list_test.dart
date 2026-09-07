// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/features/chat_details/member_selection_list.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/user/users_cubit.dart';

import '../../helpers.dart';
import '../../mocks.dart';

const _allFeatures = AirFeatures(
  encryptedGroupProfiles: true,
  emptyConnectionGroupAttributes: true,
  pqGroups: true,
);

const _noPqFeatures = AirFeatures(
  encryptedGroupProfiles: true,
  emptyConnectionGroupAttributes: true,
  pqGroups: false,
);

const _noEgpFeatures = AirFeatures(
  encryptedGroupProfiles: false,
  emptyConnectionGroupAttributes: true,
  pqGroups: true,
);

final _profiles = [
  UiUserProfile(userId: 1.userId(), displayName: 'Alice (all features)'),
  UiUserProfile(userId: 2.userId(), displayName: 'Bob (no PQ)'),
  UiUserProfile(userId: 3.userId(), displayName: 'Eve (no EGP)'),
  UiUserProfile(userId: 4.userId(), displayName: 'Charlie (no features)'),
];

final _contacts = [
  UiContact(
    userId: 1.userId(),
    chatId: 1.chatId(),
    supportedFeatures: _allFeatures,
  ),
  UiContact(
    userId: 2.userId(),
    chatId: 2.chatId(),
    supportedFeatures: _noPqFeatures,
  ),
  UiContact(
    userId: 3.userId(),
    chatId: 3.chatId(),
    supportedFeatures: _noEgpFeatures,
  ),
  UiContact(userId: 4.userId(), chatId: 4.chatId()),
];

void main() {
  setUpAll(() {
    registerFallbackValue(0.userId());
  });

  group('MemberSelectionList', () {
    late MockUsersCubit usersCubit;

    setUp(() {
      usersCubit = MockUsersCubit();
      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: _profiles));
    });

    Widget buildSubject({required bool isApq, List<UiContact>? contacts}) =>
        MultiBlocProvider(
          providers: [BlocProvider<UsersCubit>.value(value: usersCubit)],
          child: Builder(
            builder: (context) => MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: MemberSelectionList(
                  contacts: contacts ?? _contacts,
                  selectedContacts: const {},
                  query: '',
                  isApq: isApq,
                  onToggle: (_) {},
                ),
              ),
            ),
          ),
        );

    double verticalPositionOf(WidgetTester tester, String text) =>
        tester.getTopLeft(find.text(text)).dy;

    testWidgets('non-APQ chat greys out only contacts missing EGP', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject(isApq: false));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/member_selection_list_non_apq.png'),
      );
    });

    testWidgets('APQ chat greys out contacts missing EGP or PQ', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject(isApq: true));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/member_selection_list_apq.png'),
      );
    });

    testWidgets('contacts that can\'t join sit in the section at the bottom', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject(isApq: false));

      final header = verticalPositionOf(tester, "Can't be added");

      expect(
        verticalPositionOf(tester, 'Alice (all features)'),
        lessThan(header),
      );
      expect(verticalPositionOf(tester, 'Bob (no PQ)'), lessThan(header));
      expect(verticalPositionOf(tester, 'Eve (no EGP)'), greaterThan(header));
      expect(
        verticalPositionOf(tester, 'Charlie (no features)'),
        greaterThan(header),
      );
      expect(
        find.textContaining('their app version is too old'),
        findsOneWidget,
      );
    });

    testWidgets('APQ moves the contact missing PQ into the section', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject(isApq: true));

      final header = verticalPositionOf(tester, "Can't be added");

      expect(
        verticalPositionOf(tester, 'Alice (all features)'),
        lessThan(header),
      );
      expect(verticalPositionOf(tester, 'Bob (no PQ)'), greaterThan(header));
    });

    testWidgets('the section stays away when every contact can join', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(isApq: false, contacts: [_contacts.first]),
      );

      expect(find.text("Can't be added"), findsNothing);
      expect(find.text('Alice (all features)'), findsOneWidget);
    });
  });
}
