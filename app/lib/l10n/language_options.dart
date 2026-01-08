// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/app_localizations.dart';
import 'package:flutter/material.dart';

class LanguageOption {
  const LanguageOption({required this.locale, required this.label});

  final Locale locale;
  final String label;
}

List<LanguageOption> buildLanguageOptions() {
  return AppLocalizations.supportedLocales
      .map(
        (locale) => LanguageOption(
          locale: locale,
          label: lookupAppLocalizations(locale).languageName,
        ),
      )
      .toList();
}

Locale? localeForLanguageCode(String? languageCode) {
  if (languageCode == null) {
    return null;
  }
  for (final locale in AppLocalizations.supportedLocales) {
    if (locale.languageCode == languageCode) {
      return locale;
    }
  }
  return null;
}

Locale supportedLanguageLocale(Locale locale) {
  for (final supportedLocale in AppLocalizations.supportedLocales) {
    if (supportedLocale.languageCode == locale.languageCode) {
      return supportedLocale;
    }
  }
  return AppLocalizations.supportedLocales.first;
}
