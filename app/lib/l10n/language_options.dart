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

/// Parses a persisted locale tag such as "de" or "pt-PT".
///
/// Tags stored before regional variants shipped carry no region, so a tag
/// without one parses to a locale without a country code.
Locale? localeFromTag(String? tag) {
  if (tag == null || tag.isEmpty) {
    return null;
  }
  final parts = tag.split(RegExp('[-_]'));
  final languageCode = parts.first;
  if (languageCode.isEmpty) {
    return null;
  }
  final countryCode = parts.length > 1 && parts[1].isNotEmpty ? parts[1] : null;
  return Locale(languageCode, countryCode);
}

/// Serializes a locale into the tag that gets persisted.
String localeToTag(Locale locale) {
  final countryCode = locale.countryCode;
  if (countryCode == null || countryCode.isEmpty) {
    return locale.languageCode;
  }
  return '${locale.languageCode}-$countryCode';
}

/// Supported locale that [locale] should be displayed in.
///
/// A region match is tried before a language match, so that two variants of
/// one language stay apart. Falling back to the first supported locale mirrors
/// how Flutter resolves a locale that no entry matches.
Locale resolveSupportedLocale(Locale locale) {
  for (final supportedLocale in AppLocalizations.supportedLocales) {
    if (supportedLocale == locale) {
      return supportedLocale;
    }
  }
  for (final supportedLocale in AppLocalizations.supportedLocales) {
    if (supportedLocale.languageCode == locale.languageCode) {
      return supportedLocale;
    }
  }
  return AppLocalizations.supportedLocales.first;
}
