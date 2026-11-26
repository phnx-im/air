// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/components/toggle/toggle.dart';
import 'package:air/features/chat_details/safety_code_pane.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../../mocks.dart';

final chat = chats[0];

void main() {
  group('SafetyCodeScreen', () {
    setUpAll(() {
      registerFallbackValue(0.userId());
    });

    late MockUserCubit userCubit;
    late MockUsersCubit usersCubit;

    final contactId = userProfiles[1].userId;
    final dummy = intArray12.init(12345);

    setUp(() async {
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();

      when(() => userCubit.state).thenReturn(MockUiUser(id: 0));
      when(() => userCubit.safetyCodes(any()))
          .thenAnswer((_) => Future.value(dummy));
      when(() => userCubit.setContactVerified(any(), any()))
          .thenAnswer((_) async {});
      when(() => usersCubit.state)
          .thenReturn(MockUsersState(profiles: userProfiles));
    });

    Widget buildSubject() => Builder(
      builder: (context) {
        return MultiBlocProvider(
          providers: [
            BlocProvider<UserCubit>.value(value: userCubit),
            BlocProvider<UsersCubit>.value(value: usersCubit),
          ],
          child: MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testLightTheme,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: ModalScaffold(
              title: 'Safety code',
              child: SafetyCodeView(profile: userProfiles[1]),
            ),
          ),
        );
      },
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/safety_code_pane.png'),
      );
    });

    testWidgets('renders correctly (verified)', (tester) async {
      when(() => usersCubit.state).thenReturn(
        MockUsersState(profiles: userProfiles, verified: {contactId}),
      );
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/safety_code_pane_verified.png'),
      );
    });

    testWidgets('offers the toggle once the code has loaded', (tester) async {
      final code = Completer<intArray12>();
      when(() => userCubit.safetyCodes(any())).thenAnswer((_) => code.future);
      await tester.pumpWidget(buildSubject());
      await tester.pump();

      expect(find.byType(Toggle), findsNothing);

      code.complete(dummy);
      await tester.pumpAndSettle();

      expect(find.byType(Toggle), findsOneWidget);
    });

    testWidgets('the toggle marks the user as verified', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await tester.tap(find.byType(Toggle));

      verify(() => userCubit.setContactVerified(contactId, true)).called(1);
    });

    testWidgets('the row removes the mark of a verified user', (tester) async {
      when(() => usersCubit.state).thenReturn(
        MockUsersState(profiles: userProfiles, verified: {contactId}),
      );
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await tester.tap(find.text('Mark as verified'));

      verify(() => userCubit.setContactVerified(contactId, false)).called(1);
    });
  });
}
