// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/ui/typography/monospace.dart';

import 'helpers.dart';

/// The threshold for golden file comparisons to pass (between 0 and 1 as percent)
const goldenThreshold = 0.0;

/// The physical size of the screen in the test environment
const pixel8ScreenSize = Size(1080, 2400);

/// The device pixel ratio of the test environment
const pixel8DevicePixelRatio = 2.625;

Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  setUpAll(() async {
    final binding = TestWidgetsFlutterBinding.ensureInitialized();
    await _loadFonts();
    _setGoldenFileComparatorWithThreshold(goldenThreshold);
    _setPhysicalScreenSize(binding, pixel8ScreenSize, pixel8DevicePixelRatio);
  });

  await testMain();
}

Future<void> _loadFonts() async {
  final monospace = getSystemMonospaceFontFamily();
  final fonts = <String, String>{
    "MaterialIcons": "fonts/MaterialIcons-Regular.otf",
    "SourceCodeProEmbedded": "assets/fonts/SourceCodePro.ttf",
    monospace: "assets/fonts/RobotoMono-Regular.ttf",
  };
  final usesSanFrancisco = await _tryLoadSanFranciscoFont();
  if (!usesSanFrancisco) {
    fonts["Roboto"] = "assets/fonts/Roboto-Regular.ttf";
  }
  for (final entry in fonts.entries) {
    final bytes = rootBundle.load(entry.value);
    final fontLoader = FontLoader(entry.key)..addFont(bytes);
    await fontLoader.load();
  }
}

Future<bool> _tryLoadSanFranciscoFont() async {
  if (!Platform.isMacOS) {
    return false;
  }

  const sfPaths = [
    '/System/Library/Fonts/SFNS.ttf',
    '/System/Library/Fonts/SFNSText.ttf',
    '/System/Library/Fonts/SFNSDisplay.ttf',
  ];
  const familyAliases = ['SF Pro Text', 'SF Pro', 'SFNS', 'SF', 'Roboto'];

  for (final path in sfPaths) {
    final file = File(path);
    if (!file.existsSync()) {
      continue;
    }

    final bytes = file.readAsBytesSync();
    final byteData = bytes.buffer.asByteData();

    for (final family in familyAliases) {
      final loader = FontLoader(family)..addFont(Future.value(byteData));
      await loader.load();
    }

    return true;
  }

  return false;
}

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
