// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/onboarding/update_required_screen.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

void main() {
  group('VersionExpiryBanner', () {
    late MockUserCubit userCubit;

    Widget buildSubject() => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: BlocProvider<UserCubit>.value(
            value: userCubit,
            child: const UpdateRequiredScreen(child: Text('content')),
          ),
        );
      },
    );

    testWidgets('is hidden when no expiry is announced', (tester) async {
      userCubit = MockUserCubit();
      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));

      await tester.pumpWidget(buildSubject());

      expect(find.text('content'), findsOneWidget);
      expect(find.textContaining('Update Air by'), findsNothing);
    });

    testWidgets('is shown when an expiry is announced and can be dismissed', (
      tester,
    ) async {
      userCubit = MockUserCubit();
      when(() => userCubit.state).thenReturn(
        MockUiUser(id: 1, versionExpiresAt: DateTime.utc(2026, 8, 1, 12)),
      );

      await tester.pumpWidget(buildSubject());

      expect(find.text('content'), findsOneWidget);
      expect(find.textContaining('Update Air by'), findsOneWidget);

      await tester.tap(find.byIcon(Icons.close));
      await tester.pump();

      expect(find.textContaining('Update Air by'), findsNothing);
      expect(find.text('content'), findsOneWidget);
    });
  });
}
