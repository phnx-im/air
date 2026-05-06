// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/user.dart';

import '../helpers.dart';
import '../mocks.dart';
import 'invitation_codes_view_test.dart';

const physicalSize = Size(1080, 3300);

void main() {
  group('UserSettingsScreenTest', () {
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockUserSettingsCubit userSettingsCubit;
    late MockInvitationCodesCubit invitationCodesCubit;

    setUp(() async {
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      userSettingsCubit = MockUserSettingsCubit();
      invitationCodesCubit = MockInvitationCodesCubit();

      when(() => contactsCubit.state).thenReturn(
        MockUsersState(
          profiles: [UiUserProfile(userId: 1.userId(), displayName: "ellie")],
        ),
      );
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      when(() => invitationCodesCubit.state).thenReturn(
        InvitationCodesState(
          codes: [
            code('ABCD-EFGH-IJKL'),
            token(1),
            code('MNOP-QRST-UVWX', copied: true),
            token(2),
          ],
        ),
      );
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<AppLocaleCubit>(create: (_) => AppLocaleCubit()),
        BlocProvider<UserCubit>.value(value: userCubit),
        BlocProvider<UsersCubit>.value(value: contactsCubit),
        BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        BlocProvider<InvitationCodesCubit>.value(value: invitationCodesCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const UserSettingsView(),
          );
        },
      ),
    );

    testWidgets('renders correctly (no handles)', (tester) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1, usernames: []));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/user_settings_screen_no_handles.png'),
      );
    });

    testWidgets('renders correctly (some handles)', (tester) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      when(() => userCubit.state).thenReturn(
        MockUiUser(
          id: 1,
          usernames: [
            const UiUsername(plaintext: "ellie"),
            const UiUsername(plaintext: "firefly"),
          ],
        ),
      );

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/user_settings_screen_some_handles.png'),
      );
    });

    testWidgets('renders correctly (all handles)', (tester) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      when(() => userCubit.state).thenReturn(
        MockUiUser(
          id: 1,
          usernames: [
            const UiUsername(plaintext: "ellie"),
            const UiUsername(plaintext: "firefly"),
            const UiUsername(plaintext: "kiddo"),
            const UiUsername(plaintext: "ells"),
            const UiUsername(plaintext: "wolf"),
          ],
        ),
      );

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/user_settings_screen_all_handles.png'),
      );
    });
  });
}
