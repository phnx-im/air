// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/registration/multi_device_provision_screen.dart';
import 'package:air/registration/registration.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../helpers.dart';
import '../mocks.dart';

const _testSize = Size(520, 1100);

const _qrSvg = '''
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 90 90">
  <rect width="90" height="90" fill="white"/>
  <rect x="10" y="10" width="20" height="20" fill="black"/>
  <rect x="60" y="10" width="20" height="20" fill="black"/>
  <rect x="10" y="60" width="20" height="20" fill="black"/>
  <rect x="40" y="40" width="10" height="10" fill="black"/>
</svg>
''';

void main() {
  group('MultiDeviceProvisionScreen', () {
    late MockRegistrationCubit registrationCubit;
    late MockNavigationCubit navigationCubit;
    late MockMultiDeviceProvisionedUser provisionedUser;

    setUp(() {
      registrationCubit = MockRegistrationCubit();
      navigationCubit = MockNavigationCubit();
      provisionedUser = MockMultiDeviceProvisionedUser();
      when(() => provisionedUser.take()).thenReturn(MockUser());
      when(
        () => registrationCubit.state,
      ).thenReturn(const RegistrationState(domain: 'example.com'));
    });

    Widget buildSubject(Stream<MultiDeviceProvisionEvent> stream) =>
        MultiBlocProvider(
          providers: [
            BlocProvider<RegistrationCubit>.value(value: registrationCubit),
            BlocProvider<NavigationCubit>.value(value: navigationCubit),
          ],
          child: Builder(
            builder: (context) {
              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: MultiDeviceProvisionScreen(
                  provisionClient:
                      ({
                        required String domain,
                        required String dbPath,
                        required MultiDeviceProvisionedUser provisionedUser,
                      }) => stream,
                  dbPathResolver: () async => '/tmp/test-link-db',
                  provisionedUserFactory: () => provisionedUser,
                  onLinked: (_) {},
                ),
              );
            },
          ),
        );

    StreamController<MultiDeviceProvisionEvent> setUpView(WidgetTester tester) {
      tester.view.physicalSize = _testSize;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final controller = StreamController<MultiDeviceProvisionEvent>();
      addTearDown(controller.close);
      return controller;
    }

    testWidgets('connecting', (tester) async {
      final controller = setUpView(tester);

      await tester.pumpWidget(buildSubject(controller.stream));
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/multi_device_provision_connecting.png'),
      );
    });

    testWidgets('awaiting link', (tester) async {
      final controller = setUpView(tester);

      await tester.pumpWidget(buildSubject(controller.stream));
      controller.add(
        const MultiDeviceProvisionEvent.code(
          code: '1234 5678',
          qrcodeSvg: _qrSvg,
        ),
      );
      await tester.pump();
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/multi_device_provision_awaiting.png'),
      );
    });

    testWidgets('failed shows error modal', (tester) async {
      final controller = setUpView(tester);

      await tester.pumpWidget(buildSubject(controller.stream));
      // Let the (async) db-path resolve so the provisioning stream is attached
      // before we push an event into it.
      await tester.pump();
      controller.add(
        const MultiDeviceProvisionEvent.failed('The linking codes expired.'),
      );
      // Can't pumpAndSettle: the spinner behind the modal never settles.
      await tester.pump();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/multi_device_provision_failed.png'),
      );
    });
  });
}
