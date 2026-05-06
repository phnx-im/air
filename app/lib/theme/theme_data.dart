// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/palette.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/effects/cupertino_scrim_transition.dart';
import 'package:air/ui/theme/font.dart';
import 'package:air/ui/typography/font_size.dart';

ThemeData darkTheme = themeData(Brightness.dark);
ThemeData lightTheme = themeData(Brightness.light);

ThemeData themeData(Brightness brightness) {
  final baselineTheme = ThemeData(brightness: brightness);

  final colorScheme = switch (brightness) {
    Brightness.dark => darkCustomColorScheme,
    Brightness.light => lightCustomColorScheme,
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
      primary: colorScheme.text.primary,
      onPrimary: colorScheme.backgroundBase.primary,
      secondary: colorScheme.text.secondary,
      onSecondary: colorScheme.backgroundBase.primary,
      surface: colorScheme.backgroundBase.primary,
      onSurface: colorScheme.text.primary,
      error: colorScheme.function.danger,
      onError: colorScheme.text.primary,
    ),
    appBarTheme: AppBarTheme(
      backgroundColor: colorScheme.backgroundBase.primary,
      elevation: 0,
      iconTheme: IconThemeData(color: colorScheme.text.primary),
      centerTitle: true,
      toolbarHeight: isPointer() ? 100 : null,
      systemOverlayStyle: brightness == Brightness.light
          ? SystemUiOverlayStyle.dark
          : SystemUiOverlayStyle.light,
      titleTextStyle: (mergedAppBarTitleStyle ?? const TextStyle()).copyWith(
        color: colorScheme.text.primary,
        fontSize: LabelFontSize.base.size,
      ),
    ),
    scaffoldBackgroundColor: colorScheme.backgroundBase.primary,
    textTheme: customTextScheme,
    canvasColor: colorScheme.backgroundBase.primary,
    cardColor: colorScheme.backgroundBase.primary,
    dialogTheme: DialogThemeData(
      backgroundColor: colorScheme.backgroundBase.primary,
      surfaceTintColor: colorScheme.backgroundBase.primary,
    ),
    splashColor: Colors.transparent,
    highlightColor: Colors.transparent,
    hoverColor: Colors.transparent,
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: CustomOutlineButtonStyle(
        colorScheme: colorScheme,
        baselineTextTheme: baselineTheme.textTheme,
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: CustomTextButtonStyle(
        colorScheme: colorScheme,
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
      cursorColor: AppColors.blue[300],
    ),
    inputDecorationTheme: InputDecorationTheme(
      border: InputBorder.none,
      hintStyle: TextStyle(
        color: colorScheme.text.quaternary,
        fontSize: LabelFontSize.small1.size,
      ),
      focusedBorder: _textInputBorder,
      enabledBorder: _textInputBorder,
      errorBorder: _textInputBorder,
      focusedErrorBorder: _textInputBorder,
      filled: true,
      fillColor: colorScheme.backgroundBase.secondary,
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
      thumbColor: WidgetStateProperty.all(colorScheme.text.secondary),
      trackOutlineColor: WidgetStateProperty.all(colorScheme.separator.primary),
      trackColor: WidgetStateProperty.resolveWith(
        (states) => states.contains(WidgetState.selected)
            ? colorScheme.backgroundElevated.quaternary
            : Colors.transparent,
      ),
    ),
  );
}

final _textInputBorder = OutlineInputBorder(
  borderSide: const BorderSide(width: 0, style: BorderStyle.none),
  borderRadius: BorderRadius.circular(8),
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
