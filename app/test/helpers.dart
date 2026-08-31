// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'package:air/ds/foundations/breakpoint.dart';
import 'package:air/ds/material/theme_data.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/core/core.dart';
import 'package:uuid/uuid.dart';

ThemeData testThemeData(Brightness brightness) {
  final theme = themeData(brightness);
  return theme.copyWith(
    textTheme: theme.textTheme.apply(
      fontFamilyFallback: Platform.isMacOS
          ? ['Apple Color Emoji']
          : ['NotoColorEmoji'],
    ),
  );
}

/// A getter rather than a `final` because the theme it wraps resolves the
/// typescale and the appbar height from the target platform, which a test can
/// pin after this library is first loaded.
ThemeData get testLightTheme => testThemeData(.light);

/// Maps the host OS to the matching desktop [TargetPlatform] so widget goldens
/// render the same desktop code path the app ships on that OS, keeping the
/// per-platform golden variants consistent with their host.
TargetPlatform _desktopTargetPlatform() {
  if (Platform.isMacOS) return TargetPlatform.macOS;
  if (Platform.isWindows) return TargetPlatform.windows;
  return TargetPlatform.linux;
}

/// Renders a test as the desktop platform the host OS ships. Widening the
/// viewport alone only moves the `Breakpoint`, not the typescale or the density
/// that `DeviceType` drives.
///
/// A `variant:` rather than a call inside the test body: the binding verifies
/// that no foundation debug variable is still set at the end of the body, and
/// only a variant's teardown runs before that check.
TargetPlatformVariant get desktopPlatform =>
    TargetPlatformVariant.only(_desktopTargetPlatform());

/// A viewport in the small tier, where a modal takes the whole screen.
const phoneViewSize = Size(400, 900);

/// A viewport wide enough for the two-pane layout, where a modal is a card.
const desktopViewSize = Size(1200, 900);

/// Pins the test view to [size] at a 1:1 pixel ratio for the rest of the test,
/// so a breakpoint-driven layout renders the tier the size names.
void sizeView(WidgetTester tester, Size size) {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(() {
    tester.view.resetPhysicalSize();
    tester.view.resetDevicePixelRatio();
  });
}

/// Asserts that the widget [finder] resolves to runs to the bottom edge of the
/// modal around it.
///
/// That is what a non-scrolling modal body has to do: anything short of the
/// edge means the card collapsed towards its minimum instead of filling its
/// envelope, and the list inside it never got a height to scroll within.
void expectFillsModal(WidgetTester tester, Finder finder, Size viewSize) {
  final tokens = viewSize.width < Breakpoint.smallMaxWidth
      ? ModalShellTokens.phone
      : ModalShellTokens.desktop;
  // The card is centered in what the container padding leaves of the viewport,
  // which is also as tall as it gets: nothing caps it short of that.
  final cardHeight = viewSize.height - tokens.containerPadding.vertical;
  final rect = tester.getRect(finder);

  expect(rect.height, greaterThan(0));
  expect(rect.bottom, moreOrLessEquals((viewSize.height + cardHeight) / 2));
}

/// Asserts that the modal on screen takes the viewport whole, which is what it
/// does where there is no room for a card beside it.
///
/// Measured on the header rather than on [ModalShell], whose outer padding
/// fills the route either way.
void expectModalFillsViewport(WidgetTester tester, Size viewSize) {
  expect(tester.getSize(find.byType(DialogHeader)).width, viewSize.width);
}

/// Asserts that the modal on screen sits in a card inset from the viewport,
/// rather than taking it whole.
void expectModalIsCard(WidgetTester tester, Size viewSize) {
  expect(
    tester.getSize(find.byType(DialogHeader)).width,
    lessThan(viewSize.width),
  );
}

extension IntTestExtension on int {
  ChatId chatId() => ChatId(uuid: _intToUuidValue(this));

  MessageId messageId() => MessageId(uuid: _intToUuidValue(this));

  /// Convert an int to a [ClientId].
  UiUserId userId({String domain = "localhost"}) =>
      UiUserId(uuid: _intToUuidValue(this), domain: domain);

  UuidValue clientRecordId() => _intToUuidValue(this);

  AttachmentId attachmentId() => AttachmentId(uuid: _intToUuidValue(this));
}

UuidValue _intToUuidValue(int value) {
  // Convert int to 16-byte array
  final bytes = Uint8List(16)
    ..buffer.asByteData().setInt64(0, value, Endian.little);
  return UuidValue.fromByteList(bytes);
}

class LocalFileComparatorWithThreshold extends LocalFileComparator {
  LocalFileComparatorWithThreshold(super.testFile, this.threshold);

  final double threshold;

  String _platformSuffix() {
    switch (debugDefaultTargetPlatformOverride) {
      case TargetPlatform.android:
        return '.android';
      case TargetPlatform.iOS:
        return '.ios';
      case TargetPlatform.linux:
        return '.linux';
      case TargetPlatform.macOS:
        return '.macos';
      case TargetPlatform.windows:
        return '.windows';
      default:
        return '';
    }
  }

  @override
  Uri getTestUri(Uri key, int? version) {
    final path = key.toFilePath();
    final newPath = path.replaceFirst(
      RegExp(r'\.png$'),
      '${_platformSuffix()}.png',
    );
    return Uri.file(newPath);
  }

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final result = await GoldenFileComparator.compareLists(
      imageBytes,
      await getGoldenBytes(golden),
    );
    if (!result.passed && result.diffPercent < threshold) {
      if ((result.diffPercent - threshold).abs() > 0.01) {
        final diff = (result.diffPercent * 10000.0).round() / 100.0;
        // ignore: avoid_print
        print(
          "Golden file comparison passed with $diff% difference, "
          "which is more than 1%pt under the configured threshold of ${threshold * 100}%. "
          "Consider making the threshold tighter.",
        );
      }
      return true;
    } else if (!result.passed) {
      final error = await generateFailureOutput(result, golden, basedir);
      throw FlutterError(error);
    } else {
      return result.passed;
    }
  }
}
