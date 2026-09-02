// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:air/ds/foundations/color_theme.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/material/cupertino_scrim_transition.dart';
import 'package:air/ds/material/text_theme.dart';

ThemeData darkTheme = themeData(.dark);
ThemeData lightTheme = themeData(.light);

ThemeData themeData(Brightness brightness, {ColorTheme? theme}) {
  final palettes = ThemePalettes.from(theme ?? defaultColorTheme);
  final palette = switch (brightness) {
    .dark => palettes.dark,
    .light => palettes.light,
  };

  final base = ThemeData(
    extensions: [palettes],
    colorScheme: ColorScheme(
      brightness: brightness,
      primary: palette.accentBrand.primary,
      onPrimary: palette.accentBrand.onPrimary,
      secondary: palette.text.secondary,
      onSecondary: palette.backgroundBase.primary,
      surface: palette.backgroundBase.primary,
      onSurface: palette.text.primary,
      error: palette.function.danger,
      onError: palette.text.primary,
    ),
    appBarTheme: AppBarTheme(
      backgroundColor: palette.backgroundBase.primary,
      elevation: 0,
      iconTheme: IconThemeData(color: palette.text.primary),
      centerTitle: true,
      systemOverlayStyle: brightness == .light
          ? SystemUiOverlayStyle.dark
          : SystemUiOverlayStyle.light,
    ),
    scaffoldBackgroundColor: palette.backgroundBase.primary,
    textTheme: customTextScheme,
    splashColor: Colors.transparent,
    highlightColor: Colors.transparent,
    hoverColor: Colors.transparent,
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: palette.accentBrand.primary,
      selectionColor: palette.accentBrand.primary.withValues(alpha: 0.4),
    ),
    pageTransitionsTheme: PageTransitionsTheme(
      // We want a scrim for iOS and macOS to visually separate the new page
      // from the old one during the transition
      builders: {
        ...const PageTransitionsTheme().builders,
        TargetPlatform.iOS: const CupertinoScrimPageTransitionsBuilder(),
        TargetPlatform.macOS: const CupertinoScrimPageTransitionsBuilder(),
      },
    ),
  );

  return base.copyWith(
    appBarTheme: base.appBarTheme.copyWith(
      titleTextStyle: base.textTheme.bodyLarge?.copyWith(
        color: palette.text.primary,
      ),
    ),
  );
}
