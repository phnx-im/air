// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/qr_scanner/qr_scanner.dart';
import 'package:air/l10n/l10n.dart';
import 'package:camera/camera.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:logging/logging.dart';

import '../../helpers.dart';
import 'fake_scanner_camera.dart';

const _permissionDenied =
    'To scan the QR code, allow Air to use the camera in the system settings.';
const _cameraUnavailable = 'The camera is not available.';
const _payload = 'air://example.com/payload';
const _manualEntry = 'Type it in';

Future<void> _pumpApp(WidgetTester tester, Size size, Widget home) async {
  sizeView(tester, size);
  await tester.pumpWidget(
    MaterialApp(
      theme: testThemeData(Brightness.light),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      home: home,
    ),
  );
}

void main() {
  group('QrScannerView', () {
    late List<String> scanned;
    late int manualEntryTaps;

    setUp(() {
      scanned = [];
      manualEntryTaps = 0;
    });

    Future<void> pumpScanner(
      WidgetTester tester, {
      required ScannerCameraOpener openCamera,
      QrFrameDecoder<String>? decode,
    }) async {
      await _pumpApp(
        tester,
        const Size(400, 400),
        Material(
          child: QrScannerView(
            openCamera: openCamera,
            decode: decode ?? (_, _) async => null,
            onScanned: scanned.add,
            manualEntryLabel: _manualEntry,
            onManualEntry: () => manualEntryTaps += 1,
          ),
        ),
      );
      await tester.pump();
    }

    final placeholderIcon = find.byWidgetPredicate(
      (widget) => widget is AppIcon && widget.type == AppIconType.camera,
    );

    testWidgets('shows the placeholder while the camera starts', (
      tester,
    ) async {
      await pumpScanner(
        tester,
        openCamera: () => Completer<ScannerCamera?>().future,
      );

      expect(placeholderIcon, findsOneWidget);
      expect(find.text(_permissionDenied), findsNothing);
      expect(find.text(_cameraUnavailable), findsNothing);
    });

    testWidgets('shows the feed once the camera is open', (tester) async {
      final camera = FakeScannerCamera();
      await pumpScanner(tester, openCamera: () async => camera);

      expect(placeholderIcon, findsNothing);
      expect(camera.streaming, isTrue);
    });

    testWidgets('reports the first payload once and stops the frames', (
      tester,
    ) async {
      final camera = FakeScannerCamera();
      var decodes = 0;
      await pumpScanner(
        tester,
        openCamera: () async => camera,
        decode: (_, _) async {
          decodes += 1;
          return decodes == 1 ? null : _payload;
        },
      );

      camera.sendFrame();
      await tester.pump();
      expect(scanned, isEmpty);

      camera.sendFrame();
      await tester.pump();
      expect(scanned, [_payload]);
      expect(camera.disposed, isTrue);

      camera.sendFrame();
      await tester.pump();
      expect(decodes, 2);
      expect(scanned, [_payload]);
    });

    testWidgets('drops frames while a decode runs', (tester) async {
      final camera = FakeScannerCamera();
      final decodes = <Completer<String?>>[];
      await pumpScanner(
        tester,
        openCamera: () async => camera,
        decode: (_, _) {
          final pending = Completer<String?>();
          decodes.add(pending);
          return pending.future;
        },
      );

      camera.sendFrame();
      camera.sendFrame();
      camera.sendFrame();
      await tester.pump();
      expect(decodes, hasLength(1));

      decodes.single.complete(null);
      await tester.pump();
      camera.sendFrame();
      await tester.pump();
      expect(decodes, hasLength(2));
    });

    testWidgets('logs a failed decode and keeps scanning', (tester) async {
      final camera = FakeScannerCamera();
      final records = <LogRecord>[];
      final subscription = Logger.root.onRecord.listen(records.add);
      addTearDown(subscription.cancel);
      var decodes = 0;
      await pumpScanner(
        tester,
        openCamera: () async => camera,
        decode: (_, _) async {
          decodes += 1;
          if (decodes == 1) throw StateError('invalid luma frame');
          return _payload;
        },
      );

      camera.sendFrame();
      await tester.pump();
      expect(scanned, isEmpty);
      expect(
        records.where(
          (record) =>
              record.level == Level.WARNING &&
              record.error is StateError &&
              record.message == 'failed to decode a camera frame',
        ),
        hasLength(1),
      );

      camera.sendFrame();
      await tester.pump();
      expect(scanned, [_payload]);
    });

    testWidgets('shows a message when the permission is denied', (
      tester,
    ) async {
      await pumpScanner(
        tester,
        openCamera: () async =>
            throw CameraException('CameraAccessDenied', 'denied'),
      );

      expect(find.text(_permissionDenied), findsOneWidget);
      expect(placeholderIcon, findsNothing);

      await tester.tap(find.text(_manualEntry));
      expect(manualEntryTaps, 1);
    });

    testWidgets('shows a message when the device has no camera', (
      tester,
    ) async {
      await pumpScanner(tester, openCamera: () async => null);

      expect(find.text(_cameraUnavailable), findsOneWidget);
      expect(find.text(_manualEntry), findsOneWidget);
    });

    testWidgets('shows a message when the camera fails to open', (
      tester,
    ) async {
      await pumpScanner(
        tester,
        openCamera: () async =>
            throw CameraException('CameraAccessError', 'broken'),
      );

      expect(find.text(_cameraUnavailable), findsOneWidget);
    });

    testWidgets('releases the camera in the background and reopens it', (
      tester,
    ) async {
      final cameras = <FakeScannerCamera>[];
      await pumpScanner(
        tester,
        openCamera: () async {
          final camera = FakeScannerCamera();
          cameras.add(camera);
          return camera;
        },
      );
      expect(cameras, hasLength(1));

      tester.binding.handleAppLifecycleStateChanged(.inactive);
      await tester.pump();
      expect(cameras.single.disposed, isTrue);
      expect(placeholderIcon, findsOneWidget);

      tester.binding.handleAppLifecycleStateChanged(.resumed);
      await tester.pumpAndSettle();
      expect(cameras, hasLength(2));
      expect(cameras.last.streaming, isTrue);
      expect(placeholderIcon, findsNothing);
    });

    testWidgets('releases a camera that opens in the background', (
      tester,
    ) async {
      final openings = <Completer<ScannerCamera?>>[];
      await pumpScanner(
        tester,
        openCamera: () {
          final opening = Completer<ScannerCamera?>();
          openings.add(opening);
          return opening.future;
        },
      );

      tester.binding.handleAppLifecycleStateChanged(.inactive);
      final first = FakeScannerCamera();
      openings.single.complete(first);
      await tester.pump();
      expect(first.disposed, isTrue);
      expect(first.streaming, isFalse);
      expect(placeholderIcon, findsOneWidget);

      tester.binding.handleAppLifecycleStateChanged(.resumed);
      await tester.pump();
      expect(openings, hasLength(2));
      final second = FakeScannerCamera();
      openings.last.complete(second);
      await tester.pumpAndSettle();
      expect(second.streaming, isTrue);
      expect(placeholderIcon, findsNothing);
    });

    testWidgets('does not ask for a denied permission again on resume', (
      tester,
    ) async {
      var opens = 0;
      await pumpScanner(
        tester,
        openCamera: () async {
          opens += 1;
          throw CameraException('CameraAccessDenied', 'denied');
        },
      );

      tester.binding.handleAppLifecycleStateChanged(.inactive);
      tester.binding.handleAppLifecycleStateChanged(.resumed);
      await tester.pump();

      expect(opens, 1);
      expect(find.text(_permissionDenied), findsOneWidget);
    });

    testWidgets('releases a camera that opens after the scanner is gone', (
      tester,
    ) async {
      final camera = FakeScannerCamera();
      final opening = Completer<ScannerCamera?>();
      await pumpScanner(tester, openCamera: () => opening.future);

      await tester.pumpWidget(const SizedBox());
      opening.complete(camera);
      await tester.pump();

      expect(camera.disposed, isTrue);
      expect(camera.streaming, isFalse);
    });

    testWidgets('releases the camera on dispose', (tester) async {
      final camera = FakeScannerCamera();
      await pumpScanner(tester, openCamera: () async => camera);

      await tester.pumpWidget(const SizedBox());

      expect(camera.disposed, isTrue);
    });

    testWidgets('focuses where the preview is tapped', (tester) async {
      final camera = FakeScannerCamera();
      await pumpScanner(tester, openCamera: () async => camera);

      // The 3:4 preview covers the square scanner, which shows its middle
      // three quarters.
      await tester.tapAt(const Offset(200, 200));
      await tester.tapAt(const Offset(100, 0));
      await tester.pump();

      expect(camera.focusPoints, hasLength(2));
      expect(camera.focusPoints[0].dx, moreOrLessEquals(0.5));
      expect(camera.focusPoints[0].dy, moreOrLessEquals(0.5));
      expect(camera.focusPoints[1].dx, moreOrLessEquals(0.25));
      expect(camera.focusPoints[1].dy, moreOrLessEquals(0.125));
    });
  });
}
