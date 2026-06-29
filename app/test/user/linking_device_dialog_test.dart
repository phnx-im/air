// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/linking_device_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers.dart';

const _testSize = Size(600, 1000);

void main() {
  group('LinkDeviceModal linking flow', () {
    late StreamController<MultiDeviceLinkEvent> controller;

    setUp(() {
      controller = StreamController<MultiDeviceLinkEvent>();
    });
    tearDown(() => controller.close());

    // Injected in place of the real Rust-backed session: drives the linking page
    // from the test-controlled [controller]; [confirm] is a no-op since the
    // phase transition is driven by the UI.
    LinkSession fakeSession(BuildContext context, String sessionId) =>
        (events: controller.stream, confirm: () {});

    /// Pumps the modal and navigates to the linking page (which shows the
    /// "connecting" spinner). Uses [WidgetTester.pump] for the final step since
    /// the spinner never settles.
    Future<void> pumpToConnecting(WidgetTester tester) async {
      tester.view.physicalSize = _testSize;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      await tester.pumpWidget(
        Builder(
          builder: (context) {
            return MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: LinkDeviceModal(startLinkSession: fakeSession),
              ),
            );
          },
        ),
      );
      await tester.pumpAndSettle();

      // chooser -> numeric code -> enter a code -> Link device.
      await tester.tap(find.text('Enter numeric code'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField), '12345678');
      await tester.tap(find.text('Link device'));
      await tester.pump();
    }

    testWidgets('connecting', (tester) async {
      await pumpToConnecting(tester);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_link_connecting.png'),
      );
    });

    testWidgets('awaiting confirmation', (tester) async {
      await pumpToConnecting(tester);

      controller.add(const MultiDeviceLinkEvent.awaitingConfirmation());
      await tester.pump();
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_link_confirm.png'),
      );
    });

    testWidgets('linking after confirm', (tester) async {
      await pumpToConnecting(tester);

      controller.add(const MultiDeviceLinkEvent.awaitingConfirmation());
      await tester.pump();
      await tester.pump();

      // Tick the checkbox to enable the confirm button, then confirm.
      await tester.tap(find.byType(Checkbox));
      await tester.pump();
      await tester.tap(find.text('Confirm'));
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_link_linking.png'),
      );
    });

    testWidgets('failed', (tester) async {
      await pumpToConnecting(tester);

      controller.add(const MultiDeviceLinkEvent.failed('boom'));
      await tester.pump();
      await tester.pump();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_link_failed.png'),
      );
    });
  });
}
