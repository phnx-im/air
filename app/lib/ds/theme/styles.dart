// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/theme/responsive_screen.dart';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/theme/font.dart';
import 'package:flutter/material.dart';
import 'package:air/ds/foundations/themes.dart';

// === Devices ===

bool isSmallScreen(BuildContext context) {
  return MediaQuery.sizeOf(context).width < kMobileBreakpoint;
}

bool isLargeScreen(BuildContext context) {
  return MediaQuery.sizeOf(context).width >= kMobileBreakpoint;
}

// === Buttons ===

extension on CustomColorScheme {
  Color get activeButtonColor => backgroundBase.quaternary;
  Color get inactiveButtonColor => backgroundBase.secondary;
}

class CustomTextButtonStyle extends ButtonStyle {
  CustomTextButtonStyle({
    required CustomColorScheme colorScheme,
    required TextTheme baselineTextTheme,
  }) : super(
         foregroundColor: WidgetStateProperty.fromMap({
           WidgetState.disabled: colorScheme.text.quaternary,
           WidgetState.any: colorScheme.text.secondary,
         }),
         overlayColor: WidgetStateProperty.all(Colors.transparent),
         surfaceTintColor: WidgetStateProperty.all(Colors.transparent),
         splashFactory: NoSplash.splashFactory,
         padding: WidgetStateProperty.all(const EdgeInsets.all(20)),
         textStyle: WidgetStateProperty.all<TextStyle>(
           baselineTextTheme.labelMedium!.merge(customTextScheme.labelMedium!),
         ),
       );
}

class CustomOutlineButtonStyle extends ButtonStyle {
  CustomOutlineButtonStyle({
    required CustomColorScheme colorScheme,
    required TextTheme baselineTextTheme,
  }) : super(
         foregroundColor: WidgetStateProperty<Color>.fromMap({
           WidgetState.disabled: colorScheme.text.quaternary,
           WidgetState.any: colorScheme.text.primary,
         }),
         backgroundColor: WidgetStateProperty<Color>.fromMap({
           WidgetState.disabled: colorScheme.inactiveButtonColor,
           WidgetState.any: colorScheme.activeButtonColor,
         }),
         overlayColor: WidgetStateProperty<Color>.fromMap({
           WidgetState.disabled: colorScheme.inactiveButtonColor,
           WidgetState.any: colorScheme.activeButtonColor,
         }),
         mouseCursor: const WidgetStateProperty<MouseCursor>.fromMap({
           WidgetState.disabled: SystemMouseCursors.basic,
           WidgetState.any: SystemMouseCursors.click,
         }),
         elevation: WidgetStateProperty.all<double>(0),
         shadowColor: WidgetStateProperty.all<Color>(Colors.transparent),
         padding: WidgetStateProperty.all<EdgeInsetsGeometry>(
           const EdgeInsets.symmetric(
             vertical: Spacing.px16,
             horizontal: Spacing.px16,
           ),
         ),
         splashFactory: NoSplash.splashFactory,
         surfaceTintColor: WidgetStateProperty.all<Color>(Colors.transparent),
         side: WidgetStateProperty.all<BorderSide>(
           const BorderSide(color: Colors.transparent, width: 0),
         ),
         shape: WidgetStateProperty.all<OutlinedBorder>(
           RoundedRectangleBorder(
             side: const BorderSide(
               color: Colors.transparent,
               width: 0,
               style: BorderStyle.none,
             ),
             borderRadius: BorderRadius.circular(12),
           ),
         ),
         textStyle: WidgetStatePropertyAll(baselineTextTheme.labelLarge!),
       );
}
