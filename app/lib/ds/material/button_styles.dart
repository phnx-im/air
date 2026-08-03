// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/material/text_theme.dart';
import 'package:flutter/material.dart';

// === Buttons ===

extension on SemanticPalette {
  Color get activeButtonColor => backgroundBase.quaternary;
  Color get inactiveButtonColor => backgroundBase.secondary;
}

class CustomTextButtonStyle extends ButtonStyle {
  CustomTextButtonStyle({
    required SemanticPalette palette,
    required TextTheme baselineTextTheme,
  }) : super(
         foregroundColor: WidgetStateProperty.fromMap({
           WidgetState.disabled: palette.text.quaternary,
           WidgetState.any: palette.text.secondary,
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
    required SemanticPalette palette,
    required TextTheme baselineTextTheme,
  }) : super(
         foregroundColor: WidgetStateProperty<Color>.fromMap({
           WidgetState.disabled: palette.text.quaternary,
           WidgetState.any: palette.text.primary,
         }),
         backgroundColor: WidgetStateProperty<Color>.fromMap({
           WidgetState.disabled: palette.inactiveButtonColor,
           WidgetState.any: palette.activeButtonColor,
         }),
         overlayColor: WidgetStateProperty<Color>.fromMap({
           WidgetState.disabled: palette.inactiveButtonColor,
           WidgetState.any: palette.activeButtonColor,
         }),
         mouseCursor: const WidgetStateProperty<MouseCursor>.fromMap({
           WidgetState.disabled: SystemMouseCursors.basic,
           WidgetState.any: SystemMouseCursors.click,
         }),
         elevation: WidgetStateProperty.all<double>(0),
         shadowColor: WidgetStateProperty.all<Color>(Colors.transparent),
         padding: WidgetStateProperty.all<EdgeInsetsGeometry>(
           const EdgeInsets.symmetric(vertical: S.s16, horizontal: S.s16),
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
             borderRadius: BorderRadius.circular(CornerRadius.px12),
           ),
         ),
         textStyle: WidgetStatePropertyAll(baselineTextTheme.labelLarge!),
       );
}
