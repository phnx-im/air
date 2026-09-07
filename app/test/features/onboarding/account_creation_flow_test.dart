// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/onboarding/account_creation_flow.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:air/features/user/loadable_user_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:uuid/uuid.dart';

import '../../helpers.dart';
import '../../mocks.dart';

/// A code of the length the first step insists on, so the flow can be driven
/// past it.
const _validCode = 'ABCD2345';

/// A server that admits anyone without a challenge.
const _openRegistration = RegistrationInfo(
  challengeRequired: false,
  acceptedChallenges: [ChallengeKind.invitationCode],
);

/// A server that gates registration behind something this build cannot answer.
const _unsupportedRegistration = RegistrationInfo(
  challengeRequired: true,
  acceptedChallenges: [],
);

/// A gated server that takes either an admission session or a code.
const _gatedRegistration = RegistrationInfo(
  challengeRequired: true,
  acceptedChallenges: [
    ChallengeKind.invitationCode,
    ChallengeKind.admissionSession,
  ],
);

/// A gated server that takes nothing but an admission session.
const _sessionOnlyRegistration = RegistrationInfo(
  challengeRequired: true,
  acceptedChallenges: [ChallengeKind.admissionSession],
);

AdmissionSession _admissionSession() => AdmissionSession(
  sessionId: UuidValue.fromString('7f4a4d4c-0000-4000-8000-000000000001'),
  challenge: 'a1b2c3',
);

void main() {
  group('AccountCreationFlow', () {
    late MockRegistrationCubit registrationCubit;
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockLoadableUserCubit loadableUserCubit;

    setUp(() {
      registrationCubit = MockRegistrationCubit();
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      loadableUserCubit = MockLoadableUserCubit();

      // No user until the account is created between the profile and username
      // steps. Tests that reach the created-account state override this.
      when(
        () => loadableUserCubit.state,
      ).thenReturn(const LoadableUser.unloaded());
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
        BlocProvider<LoadableUserCubit>.value(value: loadableUserCubit),
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

    testWidgets('opens on the invite code', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(find.text('Enter invite code'), findsOneWidget);

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

      expect(find.text('Enter invite code'), findsOneWidget);
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

      expect(find.text('Enter invite code'), findsOneWidget);
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

    testWidgets('a rebuild after account creation resumes on the username', (
      tester,
    ) async {
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
      expect(find.text('Add a username'), findsOneWidget);

      // Creating the account loads the user, which in the app swaps the intro
      // subtree for the logged-in one and tears down this flow. Mimic that:
      // drop the widget entirely, mark the user loaded, then build a brand-new
      // flow.
      when(
        () => loadableUserCubit.state,
      ).thenReturn(LoadableUser.loaded(MockUser()));
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      // The rebuilt flow must resume on the username step. Without reading the
      // loaded user, the fresh state drops back to the first (invite code)
      // step.
      expect(find.text('Add a username'), findsOneWidget);
      expect(find.text('Enter invite code'), findsNothing);
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

    group('open registration', () {
      setUp(() {
        when(() => registrationCubit.state).thenReturn(
          const RegistrationState(
            displayName: 'Ellie',
            registrationInfo: _openRegistration,
          ),
        );
      });

      testWidgets('opens on the profile', (tester) async {
        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        expect(find.text('Create your profile'), findsOneWidget);
        expect(find.text('Enter invite code'), findsNothing);
      });

      testWidgets('the profile step has nothing to go back to', (tester) async {
        // Nothing typed, so the dismiss guard has nothing to ask about.
        when(() => registrationCubit.state).thenReturn(
          const RegistrationState(registrationInfo: _openRegistration),
        );

        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        await tester.tap(find.byType(DialogHeaderAction));
        await tester.pumpAndSettle();

        verify(() => navigationCubit.pop()).called(1);
      });

      testWidgets('signs up with no code and reaches the username', (
        tester,
      ) async {
        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        await submit(tester, 'Create');

        verify(() => registrationCubit.signUp()).called(1);
        verifyNever(() => registrationCubit.submitInvitationCode());
        expect(find.text('Add a username'), findsOneWidget);
      });

      testWidgets('a gate that closes mid-flow reroutes to the code', (
        tester,
      ) async {
        when(() => registrationCubit.signUp()).thenAnswer((_) async {
          // The cubit records the requirement before returning, which is what
          // puts the step into the flow.
          when(() => registrationCubit.state).thenReturn(
            const RegistrationState(
              displayName: 'Ellie',
              registrationInfo: RegistrationInfo(
                challengeRequired: true,
                acceptedChallenges: [ChallengeKind.invitationCode],
              ),
            ),
          );
          return const SignUpError(code: .challengeRequired);
        });

        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        await submit(tester, 'Create');

        expect(find.text('Enter invite code'), findsOneWidget);
      });
    });

    group('admission session', () {
      testWidgets('a session in hand skips the code step', (tester) async {
        when(() => registrationCubit.state).thenReturn(
          RegistrationState(
            displayName: 'Ellie',
            registrationInfo: _gatedRegistration,
            admissionSession: _admissionSession(),
            admissionExpiresAt: DateTime.now().toUtc().add(
              const Duration(minutes: 5),
            ),
          ),
        );

        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        expect(find.text('Create your profile'), findsOneWidget);
        expect(find.text('Enter invite code'), findsNothing);
      });

      testWidgets('an expired session falls back to the code', (tester) async {
        when(() => registrationCubit.state).thenReturn(
          RegistrationState(
            invitationCode: _validCode,
            registrationInfo: _gatedRegistration,
            admissionSession: _admissionSession(),
            admissionExpiresAt: DateTime.now().toUtc().subtract(
              const Duration(seconds: 1),
            ),
          ),
        );

        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        expect(find.text('Enter invite code'), findsOneWidget);
      });

      testWidgets('no session falls back to the code', (tester) async {
        when(() => registrationCubit.state).thenReturn(
          const RegistrationState(registrationInfo: _gatedRegistration),
        );

        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        expect(find.text('Enter invite code'), findsOneWidget);
      });

      /// Asking for a code the server ignores would be the wrong dead end.
      testWidgets('a server that takes nothing else is a dead end', (
        tester,
      ) async {
        when(() => registrationCubit.state).thenReturn(
          const RegistrationState(registrationInfo: _sessionOnlyRegistration),
        );

        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        expect(find.text('Update required'), findsOneWidget);
      });
    });

    testWidgets('an unanswerable challenge is a dead end', (tester) async {
      when(() => registrationCubit.state).thenReturn(
        const RegistrationState(registrationInfo: _unsupportedRegistration),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(find.text('Update required'), findsOneWidget);
      // Nothing on the step carries the flow forward.
      expect(find.text('Join Air'), findsNothing);
      expect(find.text('Create'), findsNothing);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/account_creation_unsupported.png'),
      );
    });

    testWidgets('a dead end clears once the server opens up', (tester) async {
      final states = StreamController<RegistrationState>();
      addTearDown(states.close);
      whenListen(
        registrationCubit,
        states.stream,
        initialState: const RegistrationState(
          registrationInfo: _unsupportedRegistration,
        ),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      expect(find.text('Update required'), findsOneWidget);

      states.add(const RegistrationState(registrationInfo: _openRegistration));
      await tester.pumpAndSettle();

      expect(find.text('Update required'), findsNothing);
      expect(find.text('Create your profile'), findsOneWidget);
    });
  });
}
