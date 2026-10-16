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

  group('resolveLocaleList', () {
    const simplified = Locale('zh');
    const traditional = Locale.fromSubtags(
      languageCode: 'zh',
      scriptCode: 'Hant',
    );
    const supported = <Locale>[Locale('en'), simplified, traditional];

    test('matches a scripted Chinese locale on its script', () {
      expect(
        resolveLocaleList(const [
          Locale.fromSubtags(
            languageCode: 'zh',
            scriptCode: 'Hant',
            countryCode: 'TW',
          ),
        ], supported),
        traditional,
      );
      expect(
        resolveLocaleList(const [
          Locale.fromSubtags(
            languageCode: 'zh',
            scriptCode: 'Hans',
            countryCode: 'CN',
          ),
        ], supported),
        simplified,
      );
    });

    test('restores the script of a Chinese locale reported without one', () {
      for (final region in ['TW', 'HK', 'MO']) {
        expect(
          resolveLocaleList([Locale('zh', region)], supported),
          traditional,
          reason: region,
        );
      }
    });

    test('leaves Simplified Chinese regions on the base locale', () {
      expect(
        resolveLocaleList(const [Locale('zh', 'CN')], supported),
        simplified,
      );
      expect(
        resolveLocaleList(const [Locale('zh', 'SG')], supported),
        simplified,
      );
      expect(resolveLocaleList(const [Locale('zh')], supported), simplified);
    });

    test('walks the preferred list in order', () {
      expect(
        resolveLocaleList(const [Locale('ja'), Locale('zh', 'TW')], supported),
        traditional,
      );
    });

    test('falls back to the first supported locale', () {
      expect(
        resolveLocaleList(const [Locale('ja')], supported),
        const Locale('en'),
      );
      expect(resolveLocaleList(null, supported), const Locale('en'));
    });
  });
}
