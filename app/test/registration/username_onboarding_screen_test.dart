// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/registration/registration.dart';
import 'package:air/theme/theme.dart';

import '../mocks.dart';

void main() {
  group('UsernameOnboardingScreen', () {
    late MockRegistrationCubit registrationCubit;

    setUp(() async {
      registrationCubit = MockRegistrationCubit();
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<RegistrationCubit>.value(value: registrationCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: themeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const Scaffold(body: UsernameOnboardingScreen()),
          );
        },
      ),
    );

    testWidgets('renders correctly', (tester) async {
      when(() => registrationCubit.state).thenReturn(const RegistrationState());

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/username_onboarding_screen.png'),
      );
    });
  });
}
