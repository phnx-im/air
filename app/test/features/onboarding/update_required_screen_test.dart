// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/onboarding/update_required_screen.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';

import '../../helpers.dart';

void main() {
  group('UpdateRequiredView', () {
    Widget buildSubject(TargetPlatform platform) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: UpdateRequiredView(platform: platform),
        );
      },
    );

    testWidgets(
      'renders correctly on phone',
      (tester) async {
        await tester.pumpWidget(buildSubject(defaultTargetPlatform));
        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/update_required_screen.png'),
        );
      },
      variant: const TargetPlatformVariant({
        TargetPlatform.android,
        TargetPlatform.iOS,
      }),
    );

    // Only the host's own desktop platform can be pinned, so the other desktop
    // platforms render on it with their own text.
    for (final platform in [
      TargetPlatform.macOS,
      TargetPlatform.windows,
      TargetPlatform.linux,
    ]) {
      testWidgets('renders correctly on ${platform.name}', (tester) async {
        sizeView(tester, desktopViewSize);
        await tester.pumpWidget(buildSubject(platform));
        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile(
            'goldens/update_required_screen_${platform.name.toLowerCase()}.png',
          ),
        );
      }, variant: desktopPlatform);
    }
  });
}
