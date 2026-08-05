// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:air/ds/material/button_styles.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/material/cupertino_scrim_transition.dart';
import 'package:air/ds/material/text_theme.dart';

ThemeData darkTheme = themeData(.dark);
ThemeData lightTheme = themeData(.light);

ThemeData themeData(Brightness brightness) {
  final baselineTheme = ThemeData(brightness: brightness);

  final palette = switch (brightness) {
    .dark => darkSemanticPalette,
    .light => lightSemanticPalette,
  };

  // AppBar title style
  final baseAppBarTitleStyle =
      baselineTheme.appBarTheme.titleTextStyle ??
      baselineTheme.textTheme.titleLarge;
  final mergedAppBarTitleStyle = baseAppBarTitleStyle?.merge(
    customTextScheme.labelMedium ?? const TextStyle(),
  );

  return ThemeData(
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
      systemOverlayStyle: brightness == .light
          ? SystemUiOverlayStyle.dark
          : SystemUiOverlayStyle.light,
      titleTextStyle: (mergedAppBarTitleStyle ?? const TextStyle()).copyWith(
        color: palette.text.primary,
        fontSize: typeScale.body.regular.fontSize,
      ),
    ),
    scaffoldBackgroundColor: palette.backgroundBase.primary,
    textTheme: customTextScheme,
    canvasColor: palette.backgroundBase.primary,
    cardColor: palette.backgroundBase.primary,
    dialogTheme: DialogThemeData(
      backgroundColor: palette.backgroundBase.primary,
      surfaceTintColor: palette.backgroundBase.primary,
    ),
    splashColor: Colors.transparent,
    highlightColor: Colors.transparent,
    hoverColor: Colors.transparent,
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: CustomOutlineButtonStyle(
        palette: palette,
        baselineTextTheme: baselineTheme.textTheme,
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: CustomTextButtonStyle(
        palette: palette,
        baselineTextTheme: baselineTheme.textTheme,
      ),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: ButtonStyle(
        splashFactory: NoSplash.splashFactory,
        surfaceTintColor: WidgetStateProperty.all<Color>(Colors.transparent),
        overlayColor: WidgetStateProperty.all(Colors.transparent),
      ),
    ),
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: Primitive.chromatic(Hue.blue, Shade.s300),
    ),
    inputDecorationTheme: InputDecorationTheme(
      border: InputBorder.none,
      hintStyle: typeScale.body.s.style(color: palette.text.quaternary),
      focusedBorder: _textInputBorder,
      enabledBorder: _textInputBorder,
      errorBorder: _textInputBorder,
      focusedErrorBorder: _textInputBorder,
      filled: true,
      fillColor: palette.backgroundBase.secondary,
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
    switchTheme: SwitchThemeData(
      thumbColor: WidgetStateProperty.all(palette.text.secondary),
      trackOutlineColor: WidgetStateProperty.all(palette.separator.primary),
      trackColor: WidgetStateProperty.resolveWith(
        (states) => states.contains(WidgetState.selected)
            ? palette.backgroundElevated.quaternary
            : Colors.transparent,
      ),
    ),
  );
}

final _textInputBorder = OutlineInputBorder(
  borderSide: const BorderSide(width: 0, style: .none),
  borderRadius: BorderRadius.circular(CornerRadius.px8),
);

/// Scroll behavior that matches Flutter's base [ScrollBehavior] physics:
/// bouncing on Apple platforms, clamping elsewhere.
///
/// [MaterialScrollBehavior] inherits [ScrollBehavior.getScrollPhysics] which
/// already does this, but an explicit override ensures the correct behavior
/// regardless of future Material changes and makes the intent visible.
class AppScrollBehavior extends MaterialScrollBehavior {
  const AppScrollBehavior();

  // iOS: bouncing with normal deceleration (touch flicks).
  static const _bouncingPhysics = BouncingScrollPhysics(
    parent: RangeMaintainingScrollPhysics(),
  );

  // macOS: bouncing with fast deceleration (trackpad flicks stop sooner).
  static const _bouncingDesktopPhysics = BouncingScrollPhysics(
    decelerationRate: ScrollDecelerationRate.fast,
    parent: RangeMaintainingScrollPhysics(),
  );

  static const _clampingPhysics = ClampingScrollPhysics(
    parent: RangeMaintainingScrollPhysics(),
  );

  @override
  ScrollPhysics getScrollPhysics(BuildContext context) {
    return switch (getPlatform(context)) {
      TargetPlatform.iOS => _bouncingPhysics,
      TargetPlatform.macOS => _bouncingDesktopPhysics,
      _ => _clampingPhysics,
    };
  }
}
