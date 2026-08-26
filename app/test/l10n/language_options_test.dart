// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/app_localizations.dart';
import 'package:air/l10n/language_options.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('localeFromTag', () {
    test('parses a bare language tag', () {
      expect(localeFromTag('de'), const Locale('de'));
    });

    test('parses a tag with a region, in either separator', () {
      expect(localeFromTag('pt-PT'), const Locale('pt', 'PT'));
      expect(localeFromTag('pt_PT'), const Locale('pt', 'PT'));
    });

    test('returns null for nothing to parse', () {
      expect(localeFromTag(null), isNull);
      expect(localeFromTag(''), isNull);
      expect(localeFromTag('-PT'), isNull);
    });
  });

  group('localeToTag', () {
    test('round-trips through localeFromTag', () {
      for (final locale in AppLocalizations.supportedLocales) {
        expect(localeFromTag(localeToTag(locale)), locale);
      }
    });

    test('omits an absent region', () {
      expect(localeToTag(const Locale('sv')), 'sv');
      expect(localeToTag(const Locale('pt', 'PT')), 'pt-PT');
    });
  });

  group('resolveSupportedLocale', () {
    test('prefers a region match over a language match', () {
      expect(
        resolveSupportedLocale(const Locale('pt', 'PT')),
        const Locale('pt', 'PT'),
      );
    });

    test('falls back to the base locale for an unlisted region', () {
      expect(
        resolveSupportedLocale(const Locale('pt', 'BR')),
        const Locale('pt'),
      );
      expect(
        resolveSupportedLocale(const Locale('pt', 'AO')),
        const Locale('pt'),
      );
    });

    test('resolves a bare language to its supported entry', () {
      expect(resolveSupportedLocale(const Locale('de')), const Locale('de'));
    });

    test('always returns a supported locale', () {
      expect(
        AppLocalizations.supportedLocales,
        contains(resolveSupportedLocale(const Locale('ja'))),
      );
    });
  });
}
