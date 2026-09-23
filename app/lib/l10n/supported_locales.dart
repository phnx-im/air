// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// Returns a locale list with fallback forced to the front, if it is contained
/// in the list.
List<Locale> supportedLocalesWithFallback(
  List<Locale> locales,
  Locale fallback,
) {
  bool isFallback(Locale locale) => locale == fallback;

  bool isBareFallback(Locale locale) =>
      locale.languageCode == fallback.languageCode &&
      (locale.countryCode == null || locale.countryCode!.isEmpty);

  final mutableLocales = locales.toList(growable: true);
  final hasFallback = mutableLocales.any(
    (locale) => locale.languageCode == fallback.languageCode,
  );
  if (!hasFallback) {
    return mutableLocales;
  }

  mutableLocales.retainWhere(
    (locale) => !isFallback(locale) && !isBareFallback(locale),
  );
  mutableLocales.insert(0, fallback);

  return mutableLocales;
}

/// Resolves the device's preferred locales against [supported] the way
/// [WidgetsApp] does on its own, after restoring the script of a Chinese
/// locale that arrived without one.
///
/// iOS and Android report Chinese with its script, as in `zh_Hant_TW`, and
/// the default resolution matches that on language and script. The Linux
/// embedder never reports a script and Windows does not always, so the same
/// device arrives as `zh_TW`, which would otherwise fall through to the bare
/// `zh` entry and show Simplified Chinese to a Traditional reader.
Locale resolveLocaleList(List<Locale>? preferred, Iterable<Locale> supported) {
  final withScripts = preferred?.map(_withChineseScript).toList();
  return basicLocaleListResolution(withScripts, supported);
}

/// Regions that write Chinese in the Traditional script.
const _traditionalChineseRegions = {'TW', 'HK', 'MO'};

Locale _withChineseScript(Locale locale) {
  if (locale.languageCode != 'zh' || locale.scriptCode != null) {
    return locale;
  }
  if (!_traditionalChineseRegions.contains(locale.countryCode)) {
    return locale;
  }
  return Locale.fromSubtags(
    languageCode: 'zh',
    scriptCode: 'Hant',
    countryCode: locale.countryCode,
  );
}
