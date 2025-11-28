// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/spacings.dart';
import 'package:air/ui/theme/font.dart';
import 'package:flutter/material.dart';
import 'package:air/ui/colors/themes.dart';
import 'dart:io' show Platform;

// === Devices ===

bool isSmallScreen(BuildContext context) {
  return MediaQuery.sizeOf(context).width <= 600;
}

bool isLargeScreen(BuildContext context) {
  return MediaQuery.sizeOf(context).width > 600;
}

bool isTouch() {
  return Platform.isIOS || Platform.isAndroid;
}

bool isPointer() {
  return Platform.isLinux || Platform.isMacOS || Platform.isWindows;
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
             vertical: Spacings.s,
             horizontal: Spacings.s,
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
