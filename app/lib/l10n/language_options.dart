// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/app_localizations.dart';
import 'package:air/l10n/supported_locales.dart';
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

/// Parses a persisted locale tag such as "de", "pt-PT" or "zh-Hant".
///
/// Tags stored before regional variants shipped carry no region, so a tag
/// without one parses to a locale without a country code. A four letter
/// subtag is a script, any other subtag after the language is a region.
Locale? localeFromTag(String? tag) {
  if (tag == null || tag.isEmpty) {
    return null;
  }
  final parts = tag.split(RegExp('[-_]'));
  final languageCode = parts.first;
  if (languageCode.isEmpty) {
    return null;
  }
  final subtags = parts.skip(1).where((part) => part.isNotEmpty);
  return Locale.fromSubtags(
    languageCode: languageCode,
    scriptCode: subtags.where(_isScriptSubtag).firstOrNull,
    countryCode: subtags.where((part) => !_isScriptSubtag(part)).firstOrNull,
  );
}

bool _isScriptSubtag(String subtag) => subtag.length == 4;

/// Serializes a locale into the tag that gets persisted, such as "zh-Hant".
String localeToTag(Locale locale) => locale.toLanguageTag();

/// Supported locale that [locale] should be displayed in.
///
/// Resolves the way the app itself does, so that two variants of one language
/// stay apart and a locale that no entry matches falls back to the first
/// supported locale.
Locale resolveSupportedLocale(Locale locale) =>
    resolveLocaleList([locale], AppLocalizations.supportedLocales);
