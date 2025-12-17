// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/safety_code_screen.dart';
import 'package:air/core/api/user_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/components/app_scaffold.dart';
import 'package:air/user/user_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../helpers.dart';
import '../mocks.dart';

final chat = chats[0];

void main() {
  group('SafetyCodeScreen', () {
    setUpAll(() {
      registerFallbackValue(0.userId());
    });

    late MockUserCubit userCubit;

    setUp(() async {
      userCubit = MockUserCubit();

      when(() => userCubit.state).thenReturn(MockUiUser(id: 0));
    });

    Widget buildSubject() => Builder(
      builder: (context) {
        return MultiBlocProvider(
          providers: [BlocProvider<UserCubit>.value(value: userCubit)],
          child: MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: lightTheme,
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: AppScaffold(child: SafetyCodeView(profile: userProfiles[1])),
          ),
        );
      },
    );

    testWidgets('renders correctly', (tester) async {
      final dummy = intArray12([
        12345,
        12345,
        12345,
        12345,
        12345,
        12345,
        12345,
        12345,
        12345,
        12345,
        12345,
        12345,
      ]);

      when(
        () => userCubit.safetyCodes(any()),
      ).thenAnswer((_) => Future.value(dummy));
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/safety_code_screen.png'),
      );
    });
  });
}
