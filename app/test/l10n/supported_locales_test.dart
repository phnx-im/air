// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/supported_locales.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('supportedLocalesWithFallback', () {
    const fallback = Locale('en', 'US');
    test('moves English (US) to the front and removes bare English', () {
      final locales = <Locale>[
        const Locale('de'),
        const Locale('en'),
        const Locale('fr'),
      ];

      final result = supportedLocalesWithFallback(locales, fallback);

      expect(
        result,
        orderedEquals(<Locale>[
          const Locale('en', 'US'),
          const Locale('de'),
          const Locale('fr'),
        ]),
      );
    });

    test('keeps order of non-English locales and avoids duplicates', () {
      final locales = <Locale>[
        const Locale('de'),
        const Locale('en'),
        const Locale('fr'),
      ];

      final result = supportedLocalesWithFallback(locales, fallback);

      expect(
        result,
        orderedEquals(<Locale>[
          const Locale('en', 'US'),
          const Locale('de'),
          const Locale('fr'),
        ]),
      );
    });

    test('returns original list when English is not supported', () {
      final locales = <Locale>[const Locale('de'), const Locale('fr')];

      final result = supportedLocalesWithFallback(locales, fallback);

      expect(
        result,
        orderedEquals(<Locale>[const Locale('de'), const Locale('fr')]),
      );
    });
  });
}
