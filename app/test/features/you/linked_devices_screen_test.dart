// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/icon_badge/app_icon_badge.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/you/linked_devices_cubit.dart';
import 'package:air/features/you/linked_devices_screen.dart';
import 'package:air/l10n/l10n.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';

class MockLinkedDevicesCubit extends MockCubit<LinkedDevicesState>
    implements LinkedDevicesCubit {}

const _testSize = Size(600, 1400);

UiLinkedDevice _device({
  required String name,
  required int platform,
  required DateTime linkedAt,
  bool isThisDevice = false,
  String clientId = '00000000-0000-0000-0000-000000000001',
}) => UiLinkedDevice(
  clientId: clientId,
  name: name,
  platform: platform,
  linkedAt: linkedAt,
  isThisDevice: isThisDevice,
);

/// A single device named after a platform that is not the host, so the golden
/// does not change with whatever machine records it.
LinkedDevicesState _singleDevice() => LinkedDevicesState(
  devices: [
    _device(
      name: 'iOS',
      platform: 2,
      linkedAt: DateTime.utc(2026, 1, 15, 2, 45),
      isThisDevice: true,
    ),
  ],
);

/// This device plus one sibling, which is the only shape that offers unlinking.
LinkedDevicesState _withSibling() => LinkedDevicesState(
  devices: [
    _device(
      name: 'iOS',
      platform: 2,
      linkedAt: DateTime.utc(2026, 1, 15, 2, 45),
      isThisDevice: true,
    ),
    _device(
      name: 'Linux',
      platform: 5,
      linkedAt: DateTime.utc(2026, 2, 3, 14, 22),
      clientId: '00000000-0000-0000-0000-000000000002',
    ),
  ],
);

void main() {
  group('LinkedDevicesView', () {
    late MockLinkedDevicesCubit cubit;

    setUp(() {
      cubit = MockLinkedDevicesCubit();
    });

    Future<void> pumpView(
      WidgetTester tester, {
      LinkedDevicesState? state,
    }) async {
      when(() => cubit.state).thenReturn(state ?? _singleDevice());
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
              home: BlocProvider<LinkedDevicesCubit>.value(
                value: cubit,
                child: const LinkedDevicesView(),
              ),
            );
          },
        ),
      );
      await tester.pumpAndSettle();
    }

    testWidgets('renders device list', (tester) async {
      await pumpView(tester);

      expect(find.text('This device'), findsOneWidget);
      // The count is of sibling devices, so a lone device reads as none linked.
      expect(find.text('No devices linked.'), findsOneWidget);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_screen.png'),
      );
    });

    /// Until the roster is readable per client, the list only ever holds this
    /// device, so the linked section and its hint must stay hidden.
    testWidgets('hides the linked section for a lone device', (tester) async {
      await pumpView(tester);

      expect(find.text('Linked devices'), findsNothing);
      expect(find.text('Tap a device to edit its name.'), findsNothing);
    });

    /// The count is of siblings, not of all devices, so it must not include
    /// this device.
    testWidgets('counts only sibling devices', (tester) async {
      await pumpView(
        tester,
        state: LinkedDevicesState(
          devices: [
            _device(
              name: 'Linux',
              platform: 5,
              linkedAt: DateTime.utc(2026, 1, 15),
              isThisDevice: true,
            ),
            _device(
              clientId: '00000000-0000-0000-0000-000000000002',
              name: 'iPhone',
              platform: 2,
              linkedAt: DateTime.utc(2026, 2, 3),
            ),
            _device(
              clientId: '00000000-0000-0000-0000-000000000003',
              name: 'Android tablet',
              platform: 1,
              linkedAt: DateTime.utc(2026, 3, 20),
            ),
          ],
        ),
      );

      expect(find.text('Linked devices'), findsOneWidget);
      expect(find.text('Tap a device to edit its name.'), findsOneWidget);
      expect(find.text('2 devices linked.'), findsOneWidget);
    });

    testWidgets('counts a single sibling in the singular', (tester) async {
      await pumpView(
        tester,
        state: LinkedDevicesState(
          devices: [
            _device(
              name: 'Linux',
              platform: 5,
              linkedAt: DateTime.utc(2026, 1, 15),
              isThisDevice: true,
            ),
            _device(
              clientId: '00000000-0000-0000-0000-000000000002',
              name: 'iPhone',
              platform: 2,
              linkedAt: DateTime.utc(2026, 2, 3),
            ),
          ],
        ),
      );

      expect(find.text('1 device linked.'), findsOneWidget);
    });

    testWidgets('falls back to a localized name without metadata', (
      tester,
    ) async {
      await pumpView(
        tester,
        state: LinkedDevicesState(
          devices: [
            _device(
              name: '',
              platform: 0,
              linkedAt: DateTime.utc(1970),
              isThisDevice: true,
            ),
          ],
        ),
      );

      expect(find.text('Unknown device'), findsOneWidget);
    });

    Finder badgeOfType(AppIconType type) => find.byWidgetPredicate(
      (widget) => widget is AppIconBadge && widget.type == type,
    );

    testWidgets('uses the phone badge for a mobile platform', (tester) async {
      // The default state is platform 2, iOS.
      await pumpView(tester);

      expect(badgeOfType(AppIconType.smartphone), findsOneWidget);
      expect(badgeOfType(AppIconType.laptop), findsNothing);
    });

    testWidgets('uses the laptop badge for a desktop platform', (tester) async {
      await pumpView(
        tester,
        state: LinkedDevicesState(
          devices: [
            _device(
              name: 'Linux',
              platform: 5,
              linkedAt: DateTime.utc(2026, 1, 15, 2, 45),
              isThisDevice: true,
            ),
          ],
        ),
      );

      expect(badgeOfType(AppIconType.laptop), findsOneWidget);
      expect(badgeOfType(AppIconType.smartphone), findsNothing);
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
      debugDefaultTargetPlatformOverride = TargetPlatform.android;
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

    testWidgets('submitting the edit dialog renames through the cubit', (
      tester,
    ) async {
      when(
        () => cubit.renameDevice(
          clientId: any(named: 'clientId'),
          name: any(named: 'name'),
        ),
      ).thenAnswer((_) async {});

      await pumpView(tester);

      await tester.tap(find.text('iOS'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField), 'Work phone');
      await tester.tap(find.text('Change'));
      await tester.pumpAndSettle();

      verify(
        () => cubit.renameDevice(
          clientId: '00000000-0000-0000-0000-000000000001',
          name: 'Work phone',
        ),
      ).called(1);
    });

    /// A device is unlinked from one of its siblings, so only sibling rows offer
    /// it. Offering it on this device would be a footgun.
    testWidgets('offers unlink only for sibling devices', (tester) async {
      await pumpView(tester, state: _withSibling());

      expect(find.byType(GestureDetector), findsWidgets);
      final trashIcons = find.byWidgetPredicate(
        (widget) => widget is AppIcon && widget.type == AppIconType.trash,
      );
      expect(trashIcons, findsOneWidget);
    });

    testWidgets('confirming the unlink dialog unlinks through the cubit', (
      tester,
    ) async {
      when(
        () => cubit.unlinkDevice(clientId: any(named: 'clientId')),
      ).thenAnswer((_) async {});

      await pumpView(tester, state: _withSibling());

      await tester.tap(
        find.byWidgetPredicate(
          (widget) => widget is AppIcon && widget.type == AppIconType.trash,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Unlink'));
      await tester.pumpAndSettle();

      verify(
        () => cubit.unlinkDevice(
          clientId: '00000000-0000-0000-0000-000000000002',
        ),
      ).called(1);
    });

    testWidgets('renders unlink confirm dialog', (tester) async {
      await pumpView(tester, state: _withSibling());

      await tester.tap(
        find.byWidgetPredicate(
          (widget) => widget is AppIcon && widget.type == AppIconType.trash,
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/linked_devices_unlink_confirm.png'),
      );
    });
  });
}
