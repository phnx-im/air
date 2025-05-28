// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:prototype/theme/theme.dart';
import 'package:prototype/user/user.dart';

import '../mocks.dart';

void main() {
  group('EditDisplayNameScreenTest', () {
    late MockUserCubit userCubit;

    setUp(() async {
      userCubit = MockUserCubit();
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [BlocProvider<UserCubit>.value(value: userCubit)],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: themeData(context),
            home: const EditDisplayNameScreen(),
          );
        },
      ),
    );

    testWidgets('renders correctly', (tester) async {
      when(
        () => userCubit.state,
      ).thenReturn(MockUiUser(id: 1, displayName: "ellie"));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/edit_display_name_screen.png'),
      );
    });
  });
}
