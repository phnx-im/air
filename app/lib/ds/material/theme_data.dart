// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/material/cupertino_scrim_transition.dart';
import 'package:air/ds/material/text_theme.dart';

ThemeData darkTheme = themeData(Brightness.dark);
ThemeData lightTheme = themeData(Brightness.light);

ThemeData themeData(Brightness brightness) {
  final palette = switch (brightness) {
    Brightness.dark => darkSemanticPalette,
    Brightness.light => lightSemanticPalette,
  };

  final base = ThemeData(
    colorScheme: ColorScheme(
      brightness: brightness,
      primary: palette.text.primary,
      onPrimary: palette.backgroundBase.primary,
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
      systemOverlayStyle: brightness == Brightness.light
          ? SystemUiOverlayStyle.dark
          : SystemUiOverlayStyle.light,
    ),
    scaffoldBackgroundColor: palette.backgroundBase.primary,
    textTheme: customTextScheme,
    splashColor: Colors.transparent,
    highlightColor: Colors.transparent,
    hoverColor: Colors.transparent,
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: palette.function.link,
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
