// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/ds/components/icon_badge/app_icon_badge.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/you/linked_devices_cubit.dart';
import 'package:air/features/you/linked_devices_screen.dart';
import 'package:air/features/you/linking_device_dialog.dart';
import 'package:air/l10n/l10n.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:uuid/uuid.dart';

import '../../helpers.dart';

class MockLinkedDevicesCubit extends MockCubit<LinkedDevicesState>
    implements LinkedDevicesCubit {}

const _testSize = Size(600, 1400);

const nilValue = UuidValue.fromNamespace(Namespace.nil);

UiLinkedDevice _device({
  required String name,
  required LinkedDevicePlatform platform,
  required DateTime linkedAt,
  bool isThisDevice = false,
  String clientId = "00000000-0000-0000-0000-000000000000",
}) => UiLinkedDevice(
  clientId: UuidValue.withValidation(clientId, .strictRFC9562),
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
      platform: LinkedDevicePlatform.ios,
      linkedAt: DateTime.utc(2026, 1, 15, 2, 45),
      isThisDevice: true,
    ),
  ],
);

LinkedDevicesState _mockDevices() => LinkedDevicesState(
  devices: [
    _device(
      clientId: "00000000-0000-0000-0000-000000000001",
      name: 'iPhone',
      platform: LinkedDevicePlatform.ios,
      linkedAt: DateTime.utc(2026, 1, 15, 2, 45),
      isThisDevice: true,
    ),
    _device(
      clientId: "00000000-0000-0000-0000-000000000666",
      name: 'MacBook Pro',
      platform: LinkedDevicePlatform.macos,
      linkedAt: DateTime.utc(2026, 2, 3, 14, 22),
    ),
    _device(
      clientId: '00000000-0000-0000-0000-000000000002',
      name: 'Pixel',
      platform: LinkedDevicePlatform.android,
      linkedAt: DateTime.utc(2026, 3, 20, 8, 10),
    ),
    _device(
      clientId: '00000000-0000-0000-0000-000000000003',
      name: '',
      platform: LinkedDevicePlatform.unknown,
      linkedAt: DateTime.utc(2026, 4, 12, 18, 30),
    ),
  ],
);

/// This device plus one sibling, which is the only shape that offers unlinking.
LinkedDevicesState _withSibling() => LinkedDevicesState(
  devices: [
    _device(
      name: 'iOS',
      platform: LinkedDevicePlatform.ios,
      linkedAt: DateTime.utc(2026, 1, 15, 2, 45),
      isThisDevice: true,
    ),
    _device(
      name: 'Linux',
      platform: LinkedDevicePlatform.linux,
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
              home: Builder(
                builder: (context) => AppScaffold(
                  title: AppLocalizations.of(
                    context,
                  ).userSettingsScreen_devices,
                  backgroundColor: SemanticPalette.of(
                    context,
                  ).backgroundBase.primary,
                  child: BlocProvider<LinkedDevicesCubit>.value(
                    value: cubit,
                    child: const SingleChildScrollView(
                      child: LinkedDevicesView(),
                    ),
                  ),
                ),
              ),
            );
          },
        ),
      );
      await tester.pumpAndSettle();
    }

    Future<void> pumpDialog(
      WidgetTester tester,
      WidgetBuilder dialogBuilder,
    ) async {
      tester.view.physicalSize = _testSize;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      await tester.pumpWidget(
        Builder(
          builder: (context) => MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: Builder(builder: dialogBuilder),
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    Future<void> expectGolden(WidgetTester tester, String fileName) =>
        expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/$fileName.png'),
        );

    testWidgets('renders device list', (tester) async {
      await pumpView(tester, state: _mockDevices());

      expect(find.text('This device'), findsOneWidget);
      expect(find.text('Linked devices'), findsOneWidget);
      expect(find.text('MacBook Pro'), findsOneWidget);
      expect(find.text('Pixel'), findsOneWidget);
      expect(find.text('Unknown device'), findsOneWidget);
      expect(find.text('3 devices linked.'), findsOneWidget);
      expect(
        find.byWidgetPredicate(
          (widget) =>
              widget is AppIconBadge && widget.type == AppIconType.smartphone,
        ),
        findsNWidgets(2),
      );
      expect(
        find.byWidgetPredicate(
          (widget) =>
              widget is AppIconBadge && widget.type == AppIconType.laptop,
        ),
        findsNWidgets(2),
      );
      expect(
        find.byWidgetPredicate(
          (widget) => widget is AppIcon && widget.type == AppIconType.trash,
        ),
        findsNWidgets(3),
      );

      await expectGolden(tester, 'linked_devices_screen');
    });

    testWidgets('renders link modal chooser page', (tester) async {
      await pumpDialog(tester, (_) => const LinkDeviceModal());

      await expectGolden(tester, 'linked_devices_link_chooser');
    });

    testWidgets('renders link modal scan QR page', (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.android;
      try {
        await pumpDialog(tester, (_) => const LinkDeviceModal());

        await tester.tap(find.text('Scan QR code'));
        await tester.pumpAndSettle();

        await expectGolden(tester, 'linked_devices_link_scan_qr');
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    });

    testWidgets('renders link modal numeric code page', (tester) async {
      await pumpDialog(tester, (_) => const LinkDeviceModal());

      await tester.tap(find.text('Enter numeric code'));
      await tester.pumpAndSettle();

      await expectGolden(tester, 'linked_devices_link_numeric_code');
    });

    testWidgets('renders edit device name dialog', (tester) async {
      await pumpDialog(
        tester,
        (_) => LinkedDeviceNameDialog(initialValue: 'iOS', onSubmit: (_) {}),
      );

      await expectGolden(tester, 'linked_devices_edit_name');
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
          clientId: UuidValue.withValidation(
            '00000000-0000-0000-0000-000000000001',
            .strictRFC9562,
          ),
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
          clientId: UuidValue.withValidation(
            '00000000-0000-0000-0000-000000000002',
            .strictRFC9562,
          ),
        ),
      ).called(1);
    });

    testWidgets('renders unlink confirm dialog', (tester) async {
      await pumpDialog(
        tester,
        (_) => UnlinkLinkedDeviceDialog(onConfirm: () {}),
      );

      await expectGolden(tester, 'linked_devices_unlink_confirm');
    });
  });
}
