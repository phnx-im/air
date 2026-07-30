// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:bloc_test/bloc_test.dart';
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

    testWidgets('read receipts toggle debounces into one submit', (
      tester,
    ) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1, usernames: []));
      when(
        () => userSettingsCubit.setReadReceipts(value: any(named: 'value')),
      ).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());

      // Three quick taps: the switch follows each tap right away, but nothing
      // is submitted while the taps keep coming.
      final toggle = find.text('Read receipts');
      await tester.tap(toggle);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.tap(toggle);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.tap(toggle);
      verifyNever(
        () => userSettingsCubit.setReadReceipts(value: any(named: 'value')),
      );

      // The quiet period elapses: exactly one submit, with the final position
      // (on -> off -> on -> off).
      await tester.pump(const Duration(milliseconds: 500));
      verify(() => userSettingsCubit.setReadReceipts(value: false)).called(1);
      verifyNever(() => userSettingsCubit.setReadReceipts(value: true));
    });

    testWidgets('pending toggle is submitted when the screen closes', (
      tester,
    ) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1, usernames: []));
      when(
        () => userSettingsCubit.setReadReceipts(value: any(named: 'value')),
      ).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());
      await tester.tap(find.text('Read receipts'));

      // The screen goes away before the debounce delay elapses. The pending
      // submit is flushed, not dropped.
      await tester.pumpWidget(const SizedBox());
      verify(() => userSettingsCubit.setReadReceipts(value: false)).called(1);
    });

    testWidgets('a submit failing after the screen closed is not an error', (
      tester,
    ) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1, usernames: []));
      when(
        () => userSettingsCubit.setReadReceipts(value: any(named: 'value')),
      ).thenAnswer(
        (_) => Future<void>.delayed(
          const Duration(milliseconds: 100),
          () => throw Exception('failed to set read receipts'),
        ),
      );

      await tester.pumpWidget(buildSubject());
      await tester.tap(find.text('Read receipts'));

      // Closing the screen flushes the pending submit, so the failure arrives
      // after the switch state is gone. There is nothing left to revert, and
      // trying to revert it anyway would throw.
      await tester.pumpWidget(const SizedBox());
      verify(() => userSettingsCubit.setReadReceipts(value: false)).called(1);

      await tester.pump(const Duration(milliseconds: 200));
      expect(tester.takeException(), isNull);
    });

    testWidgets('a settings state update does not feed back into a submit', (
      tester,
    ) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1, usernames: []));
      // A sibling device turns read receipts off after the first build.
      whenListen(
        userSettingsCubit,
        Stream.fromIterable([const UserSettings(readReceipts: false)]),
        initialState: const UserSettings(),
      );

      await tester.pumpWidget(buildSubject());
      // Deliver the update and wait out the debounce window: the switch
      // converges onto the new state without submitting anything.
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 600));
      verifyNever(
        () => userSettingsCubit.setReadReceipts(value: any(named: 'value')),
      );
    });

    testWidgets('a settings update does not replace a pending user choice', (
      tester,
    ) async {
      tester.view.physicalSize = physicalSize;
      addTearDown(() {
        tester.view.resetPhysicalSize();
      });

      when(() => userCubit.state).thenReturn(MockUiUser(id: 1, usernames: []));
      when(
        () => userSettingsCubit.setReadReceipts(value: any(named: 'value')),
      ).thenAnswer((_) async {});
      final settings = StreamController<UserSettings>();
      addTearDown(settings.close);
      whenListen(
        userSettingsCubit,
        settings.stream,
        initialState: const UserSettings(),
      );

      await tester.pumpWidget(buildSubject());

      // The user turns the setting off and then back on. Before the debounce
      // delay elapses, a sibling update moves the displayed switch to off.
      final toggle = find.text('Read receipts');
      await tester.tap(toggle);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.tap(toggle);
      settings.add(const UserSettings(readReceipts: false));
      await tester.pump();

      // The pending submit still carries the user's final choice from the
      // second tap, rather than the later programmatic switch value.
      await tester.pump(const Duration(milliseconds: 500));
      verify(() => userSettingsCubit.setReadReceipts(value: true)).called(1);
      verifyNever(() => userSettingsCubit.setReadReceipts(value: false));
    });
  });
}
