// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/app_localizations.dart';
import 'package:air/l10n/intl_locale.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';

void main() {
  setUpAll(initializeDateFormatting);

  test('formats Traditional Chinese with the Taiwan symbols', () {
    const traditional = Locale.fromSubtags(
      languageCode: 'zh',
      scriptCode: 'Hant',
    );
    expect(intlLocaleName(traditional), 'zh_TW');
    expect(
      DateFormat.EEEE(intlLocaleName(traditional))
          .format(DateTime(2023, 12, 11)),
      '星期一',
    );
  });

  test('keeps every other locale under its own name', () {
    expect(intlLocaleName(const Locale('de')), 'de');
    expect(intlLocaleName(const Locale('en', 'US')), 'en_US');
    expect(intlLocaleName(const Locale('pt', 'PT')), 'pt_PT');
  });

  test('every supported locale has date symbols', () {
    for (final locale in AppLocalizations.supportedLocales) {
      expect(
        DateFormat.localeExists(intlLocaleName(locale)),
        isTrue,
        reason: locale.toString(),
      );
    }
  });
}
