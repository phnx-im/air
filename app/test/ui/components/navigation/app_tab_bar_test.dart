// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ui/components/navigation/app_tab_bar.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../../helpers.dart';
import '../../../mocks.dart';

void main() {
  group('AppTabBar', () {
    late MockNavigationCubit navigationCubit;
    late MockUsersCubit usersCubit;

    setUp(() {
      navigationCubit = MockNavigationCubit();
      usersCubit = MockUsersCubit();

      when(() => usersCubit.state).thenReturn(
        MockUsersState(
          profiles: [UiUserProfile(userId: 1.userId(), displayName: 'Alice')],
        ),
      );
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
        BlocProvider<UsersCubit>.value(value: usersCubit),
      ],
      child: Builder(
        builder: (context) => MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const Scaffold(
            backgroundColor: Color(0xFFEEEEEE),
            body: Center(child: AppTabBar()),
          ),
        ),
      ),
    );

    void useTab(HomeTab tab) {
      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(activeTab: tab)),
      );
    }

    testWidgets('renders with chats tab active', (tester) async {
      useTab(HomeTab.chats);
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/app_tab_bar_chats.png'),
      );
    });

    testWidgets('renders with profile tab active', (tester) async {
      useTab(HomeTab.profile);
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/app_tab_bar_profile.png'),
      );
    });

    testWidgets('renders with chats tab active (dark mode)', (tester) async {
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);

      useTab(HomeTab.chats);
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/app_tab_bar_chats_dark.png'),
      );
    });
  });
}
