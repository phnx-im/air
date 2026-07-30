// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/device_type.dart';
import 'package:air/ds/foundations/type_scale.dart';
import 'package:air/ds/material/theme_data.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('DeviceType', () {
    test('classifies every target platform', () {
      for (final platform in TargetPlatform.values) {
        expect(
          () => DeviceType.fromTargetPlatform(platform),
          returnsNormally,
          reason: '${platform.name} is unclassified',
        );
      }
    });

    test('is phone under the test default', () {
      expect(defaultTargetPlatform, TargetPlatform.android);
      expect(DeviceType.current, DeviceType.phone);
    });

    // The whole point of deriving the device type from the target platform:
    // goldens depicting another platform pin one knob and get the matching
    // density and typescale, on whatever host records them.
    testWidgets(
      'follows the pinned target platform',
      (tester) async {
        expect(DeviceType.current, DeviceType.desktop);
        expect(typeScale.body.regular.fontSize, 14);
      },
      variant: TargetPlatformVariant.only(TargetPlatform.macOS),
    );

    testWidgets(
      'carries the iOS typescale on a desktop host',
      (tester) async {
        expect(DeviceType.current, DeviceType.phone);
        expect(typeScale.body.regular.fontSize, 17);
      },
      variant: TargetPlatformVariant.only(TargetPlatform.iOS),
    );

    // The theme resolves both axes at build time, so it only tracks the pinned
    // platform as long as it is not cached across platforms.
    testWidgets(
      'reaches the theme it is built into',
      (tester) async {
        final theme = themeData(Brightness.light);

        expect(theme.appBarTheme.toolbarHeight, 100);
        expect(theme.textTheme.bodyLarge?.fontSize, 14);
      },
      variant: TargetPlatformVariant.only(TargetPlatform.macOS),
    );
  });
}
