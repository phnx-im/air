// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:prototype/developer/developer.dart';
import 'package:prototype/theme/theme.dart';
import 'package:prototype/user/user.dart';
import 'package:uuid/uuid.dart';

import '../mocks.dart';

const deviceToken = "87aae6872b144c82f5aa3abf0a9eec640"
    "bd8a0937e4d3486fa67bc22f913ebc04c"
    "53550eb2a326dac9bb16051ffc75b622b"
    "d467eb4cb3606acb69468e4df4414";

void main() {
  group('DeveloperSettingsScreen', () {
    late MockUser user;
    late MockLoadableUserCubit loadableUserCubit;

    setUp(() async {
      user = MockUser();
      loadableUserCubit = MockLoadableUserCubit();

      when(() => user.userName).thenReturn("alice@localhost");
      when(() => user.clientId).thenReturn(
          UuidValue.fromString("7c19e63f-b636-4808-a034-0b7cdb462bce"));
      when(() => loadableUserCubit.state).thenReturn(LoadableUser.loaded(user));
    });

    Widget buildSubject() => MultiBlocProvider(
          providers: [
            BlocProvider<LoadableUserCubit>.value(
              value: loadableUserCubit,
            ),
          ],
          child: Builder(
            builder: (context) {
              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: themeData(context),
                home: DeveloperSettingsScreenView(
                  deviceToken: deviceToken,
                  isMobile: true,
                  onRefreshPushToken: () {},
                ),
              );
            },
          ),
        );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/developer_settings_screen.png'),
      );
    });
  });
}
