// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/linked_devices_screen.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers.dart';

const _testSize = Size(600, 1400);

void main() {
  group('LinkedDevicesView', () {
    Future<void> pumpView(WidgetTester tester) async {
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
              home: const LinkedDevicesView(),
            );
          },
        ),
      );
      await tester.pumpAndSettle();
    }

    testWidgets('renders device list', (tester) async {
      await pumpView(tester);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_screen.png'),
      );
    });

    testWidgets('renders link modal chooser page', (tester) async {
      await pumpView(tester);

      await tester.tap(find.text('Link a device'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_link_chooser.png'),
      );
    });

    testWidgets('renders link modal scan QR page', (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.linux;
      try {
        await pumpView(tester);

        await tester.tap(find.text('Link a device'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Scan QR code'));
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/linked_devices_link_scan_qr.png'),
        );
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    });

    testWidgets('renders link modal numeric code page', (tester) async {
      await pumpView(tester);

      await tester.tap(find.text('Link a device'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Enter numeric code'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_link_numeric_code.png'),
      );
    });

    testWidgets('renders edit device name dialog', (tester) async {
      await pumpView(tester);

      await tester.tap(find.text('iOS'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_edit_name.png'),
      );
    });

    testWidgets('renders unlink confirmation dialog', (tester) async {
      await pumpView(tester);

      final trash = find.byWidgetPredicate(
        (widget) => widget is AppIcon && widget.type == AppIconType.trash,
      );
      await tester.tap(trash.first);
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_unlink_confirm.png'),
      );
    });
  });
}
