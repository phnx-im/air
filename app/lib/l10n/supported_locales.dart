// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// Returns a locale list with en_US forced to the front for fallback selection.
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
