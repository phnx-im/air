// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/onboarding/account_creation_flow.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

/// A code of the length the first step insists on, so the flow can be driven
/// past it.
const _validCode = 'ABCD2345';

void main() {
  group('AccountCreationFlow', () {
    late MockRegistrationCubit registrationCubit;
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;

    setUp(() {
      registrationCubit = MockRegistrationCubit();
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();

      when(() => navigationCubit.state).thenReturn(
        const NavigationState.intro(screens: [IntroScreenType.accountCreation]),
      );
      when(() => navigationCubit.pop()).thenReturn(true);
      when(
        () => registrationCubit.state,
      ).thenReturn(const RegistrationState(invitationCode: _validCode));
      when(
        () => registrationCubit.submitInvitationCode(),
      ).thenAnswer((_) async => null);
      when(() => registrationCubit.signUp()).thenAnswer((_) async => null);
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<RegistrationCubit>.value(value: registrationCubit),
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
        BlocProvider<UserCubit>.value(value: userCubit),
      ],
      child: Builder(
        builder: (context) => MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const AccountCreationFlow(),
        ),
      ),
    );

    /// Taps the step's own call to action, whatever it is labelled.
    Future<void> submit(WidgetTester tester, String label) async {
      await tester.tap(find.text(label));
      await tester.pumpAndSettle();
    }

    testWidgets('opens on the invitation code', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(find.text('Enter invitation code'), findsOneWidget);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/account_creation_code.png'),
      );
    });

    testWidgets('a valid code moves on to the profile', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await submit(tester, 'Join Air');

      verify(() => registrationCubit.submitInvitationCode()).called(1);
      expect(find.text('Create your profile'), findsOneWidget);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/account_creation_profile.png'),
      );
    });

    testWidgets('a rejected code stays on the step', (tester) async {
      when(
        () => registrationCubit.submitInvitationCode(),
      ).thenAnswer((_) async => const CheckInvitationCodeError(code: .invalid));

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await submit(tester, 'Join Air');

      expect(find.text('Enter invitation code'), findsOneWidget);
    });

    testWidgets('a short code never reaches the server', (tester) async {
      when(
        () => registrationCubit.state,
      ).thenReturn(const RegistrationState(invitationCode: 'ABC'));

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await submit(tester, 'Join Air');

      verifyNever(() => registrationCubit.submitInvitationCode());
      expect(find.text('Code must be 8 characters'), findsOneWidget);
    });

    testWidgets('an invalid domain never reaches the server', (tester) async {
      // The field that carries the domain is hidden, so this is a value the
      // step validates without rendering it.
      when(() => registrationCubit.state).thenReturn(
        const RegistrationState(
          invitationCode: _validCode,
          domain: 'not a host',
        ),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await submit(tester, 'Join Air');

      verifyNever(() => registrationCubit.submitInvitationCode());
    });

    testWidgets('the profile step goes back to the code', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await submit(tester, 'Join Air');

      await tester.tap(find.byType(DialogHeaderAction));
      await tester.pumpAndSettle();

      expect(find.text('Enter invitation code'), findsOneWidget);
      verifyNever(() => navigationCubit.pop());
    });

    testWidgets('closing the first step drops the flow', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await tester.tap(find.byType(DialogHeaderAction));
      await tester.pumpAndSettle();

      verify(() => navigationCubit.pop()).called(1);
    });

    testWidgets('a created account moves on to the username', (tester) async {
      when(() => registrationCubit.state).thenReturn(
        const RegistrationState(
          invitationCode: _validCode,
          displayName: 'Ellie',
        ),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await submit(tester, 'Join Air');

      await submit(tester, 'Create');

      verify(() => registrationCubit.signUp()).called(1);
      expect(find.text('Add a username'), findsOneWidget);
      // The account exists behind this step, so nothing on it goes back.
      expect(find.byType(DialogHeaderAction), findsNothing);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/account_creation_username.png'),
      );
    });

    testWidgets('skipping the username enters the app', (tester) async {
      when(() => registrationCubit.state).thenReturn(
        const RegistrationState(
          invitationCode: _validCode,
          displayName: 'Ellie',
        ),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await submit(tester, 'Join Air');
      await submit(tester, 'Create');

      await submit(tester, 'Next');

      verify(() => navigationCubit.openHome()).called(1);
    });

    testWidgets('renders correctly on desktop', (tester) async {
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/account_creation_code_desktop.png'),
      );
    }, variant: desktopPlatform);
  });
}
