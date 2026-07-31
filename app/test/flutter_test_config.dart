// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/ds/foundations/foundations.dart';

import 'helpers.dart';

/// The threshold for golden file comparisons to pass (between 0 and 1 as percent)
const goldenThreshold = 0.0;

/// The physical size of the screen in the test environment. It pairs with the
/// `TargetPlatform.android` that `defaultTargetPlatform` reports under
/// `FLUTTER_TEST`, which is what drives the typescale and `DeviceType`. Tests
/// depicting another platform pin it with a `TargetPlatformVariant`.
const pixel8ScreenSize = Size(1080, 2400);

/// The device pixel ratio of the test environment
const pixel8DevicePixelRatio = 2.625;

Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  setUpAll(() async {
    final binding = TestWidgetsFlutterBinding.ensureInitialized();
    _mockSystemDateTimeFormatChannel(binding);
    await _loadFonts();
    _setGoldenFileComparatorWithThreshold(goldenThreshold);
    _setPhysicalScreenSize(binding, pixel8ScreenSize, pixel8DevicePixelRatio);
  });

  await testMain();
}

/// Every family Material's `Typography` selects, keyed by the platform that
/// picks it. The typescale leaves `fontFamily` unset, so the theme fills in the
/// family of the platform a test pins, and a family with no font registered
/// renders every glyph as a filled box.
const _typographyFamilies = <String>[
  'Roboto', // android, fuchsia, linux
  '.AppleSystemUIFont', // macOS
  'Segoe UI', // windows
  'CupertinoSystemText', // iOS
  'CupertinoSystemDisplay', // iOS
];

Future<void> _loadFonts() async {
  final monospaceFamily = getSystemMonospaceFontFamily();
  // Load MaterialIcons from the Flutter SDK via rootBundle
  final iconBytes = rootBundle.load("fonts/MaterialIcons-Regular.otf");
  final iconLoader = FontLoader("MaterialIcons")..addFont(iconBytes);
  await iconLoader.load();

  // Load test-only fonts from disk (not registered in pubspec.yaml)
  await _loadFont("NotoEmoji", _readFont("test/fonts/NotoEmoji.ttf"));

  final monospace =
      _readSystemFont(_systemMonospacePaths) ??
      _readFont("test/fonts/RobotoMono-Regular.ttf");
  await _loadFont(monospaceFamily, monospace);

  // Every family gets the same face so that a host renders its whole golden set
  // in one font, whichever platform a test pins.
  final text =
      _readSystemFont(_systemUiFontPaths) ??
      _readFont("test/fonts/Roboto-Regular.ttf");
  for (final family in _typographyFamilies) {
    await _loadFont(family, text);
  }
}

Future<void> _loadFont(String family, ByteData bytes) async {
  final loader = FontLoader(family)..addFont(Future.value(bytes));
  await loader.load();
}

ByteData _readFont(String path) =>
    File(path).readAsBytesSync().buffer.asByteData();

/// The first of [paths] the host actually ships, or null when it ships none.
ByteData? _readSystemFont(List<String> paths) {
  for (final path in paths) {
    final file = File(path);
    if (file.existsSync()) {
      return file.readAsBytesSync().buffer.asByteData();
    }
  }

  return null;
}

List<String> get _systemMonospacePaths => <String>[
  if (Platform.isMacOS || Platform.isIOS) ...[
    '/System/Library/Fonts/Menlo.ttc',
    '/Library/Fonts/Menlo.ttc',
  ],
  if (Platform.isWindows) ...[
    r'C:\Windows\Fonts\consola.ttf',
    r'C:\Windows\Fonts\consolab.ttf',
    r'C:\Windows\Fonts\consolai.ttf',
    r'C:\Windows\Fonts\consolaz.ttf',
  ],
];

/// Only macOS contributes a path: San Francisco is the one system UI font we
/// can read from a stable location. Windows and Linux fall back to the
/// in-source Roboto so their goldens do not drift with an OS font update.
List<String> get _systemUiFontPaths => <String>[
  if (Platform.isMacOS || Platform.isIOS) ...[
    '/System/Library/Fonts/SFNS.ttf',
    '/System/Library/Fonts/SFNSText.ttf',
    '/System/Library/Fonts/SFNSDisplay.ttf',
  ],
];

void _setGoldenFileComparatorWithThreshold(double threshold) {
  assert(goldenFileComparator is LocalFileComparator);
  final testUrl = (goldenFileComparator as LocalFileComparator).basedir;
  goldenFileComparator = LocalFileComparatorWithThreshold(
    // only the base dir is used from this URI, so pass a dummy file name
    Uri.parse('$testUrl/test.dart'),
    threshold,
  );
}

void _setPhysicalScreenSize(
  TestWidgetsFlutterBinding binding,
  Size screenSize,
  double devicePixelRatio,
) {
  binding.platformDispatcher.views.first.physicalSize = screenSize;
  binding.platformDispatcher.views.first.devicePixelRatio = devicePixelRatio;
  addTearDown(() {
    binding.platformDispatcher.views.first.resetPhysicalSize();
  });
}

void _mockSystemDateTimeFormatChannel(TestWidgetsFlutterBinding binding) {
  binding.defaultBinaryMessenger.setMockMethodCallHandler(
    const MethodChannel('system_date_time_format'),
    (MethodCall methodCall) async => null,
  );
}
