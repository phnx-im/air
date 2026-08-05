// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ds/foundations/semantic_colors.dart';

/// Selects between the system-following theme and a themed variant that is
/// fixed regardless of system brightness. Developer-only for now, toggled
/// from `DeveloperSettingsScreen`.
enum AppThemeChoice { standard, frutigerAero }

/// Publishes the current [AppThemeChoice] down the tree so
/// [SemanticPalette.of] can resolve to the right palette without every
/// caller threading the choice through explicitly.
class AppThemeScope extends InheritedWidget {
  const AppThemeScope({required this.choice, required super.child, super.key});

  final AppThemeChoice choice;

  static AppThemeChoice of(BuildContext context) {
    return context
            .dependOnInheritedWidgetOfExactType<AppThemeScope>()
            ?.choice ??
        AppThemeChoice.standard;
  }

  @override
  bool updateShouldNotify(AppThemeScope oldWidget) =>
      choice != oldWidget.choice;
}

// Windows XP Luna Blue: the saturated title-bar blue, the green Start
// button, and the tan dialog chrome that defined the era. A single fixed
// palette rather than a light/dark pair, since Luna never had a dark mode.

const _xpBlue = Color(0xFF0054E3); // title bar / Start button highlight
const _xpBlueBright = Color(0xFF3D95FF); // title bar gradient top
const _xpBlueDark = Color(0xFF1941A5); // title bar gradient bottom
const _xpGreen = Color(0xFF3C7C0F); // Start button, dark end of its gradient
const _xpGreenBright = Color(0xFF6FBE45); // Start button, bright end
const _xpRed = Color(0xFFC42B1C); // close-button red
const _xpYellow = Color(0xFFFFC20E); // warning triangle
const _xpTan = Color(0xFFECE9D8); // classic dialog/control face
const _xpTanDark = Color(0xFFACA899); // bevel border on tan
const _xpInk = Color(0xFF101010); // near-black dialog text
const _xpInkMuted = Color(0xFF404040);

/// The classic text-selection highlight blue, distinct from the chrome blue
/// above: `SelectedItems=49 106 197` in the old `.theme`/registry color
/// scheme dumps. Public so `theme_data.dart` can wire it into
/// [TextSelectionThemeData] for this palette specifically.
const xpSelectionBlue = Color(0xFF316AC5);

final SemanticPalette aeroSemanticPalette = SemanticPalette(
  accentBrand: const AccentBrand(
    primary: _xpBlue,
    secondary: _xpBlueBright,
    tertiary: _xpGreenBright,
    quaternary: _xpTan,
  ),
  backgroundBase: const BackgroundBase(
    primary: _xpTan,
    secondary: Color(0xFFDDD8C0),
    tertiary: Colors.white,
    quaternary: Color(0xFFD6E4F7),
    quinary: Colors.white,
  ),
  backgroundElevated: const BackgroundElevated(
    primary: Colors.white,
    secondary: Color(0xFFF5F3E7),
    tertiary: Colors.white,
    quaternary: Color(0xFFD6E4F7),
    quinary: Color(0xFFF5F3E7),
  ),
  backgroundMaterial: BackgroundMaterial(
    primary: Colors.white.withValues(alpha: 0.6),
    secondary: _xpTan.withValues(alpha: 0.55),
    tertiary: Colors.white.withValues(alpha: 0.7),
    quaternary: _xpTan.withValues(alpha: 0.45),
  ),
  text: const TextPalette(
    primary: _xpInk,
    secondary: _xpInkMuted,
    tertiary: Color(0xFF6E6E6E),
    quaternary: Color(0xFF9A9A9A),
  ),
  separator: const SeparatorPalette(primary: _xpTanDark, secondary: _xpTan),
  fill: FillPalette(
    primary: _xpBlue.withValues(alpha: 0.15),
    // Backs the in-bubble reply quote (see ReplyBlock). Needs to read against
    // both the tan "other" bubble and the saturated blue "self" bubble, so
    // it's a near-opaque tan rather than a blue tint that would all but
    // vanish into a blue background.
    secondary: _xpTan.withValues(alpha: 0.92),
    tertiary: _xpBlue.withValues(alpha: 0.05),
  ),
  function: FunctionPalette(
    neutral: FunctionNeutral(
      white: Colors.white,
      black: _xpInk,
      toggleWhite: Colors.white,
      toggleBlack: _xpInk,
      scrim: _xpInk.withValues(alpha: 0.2),
      scrimDark: _xpInk.withValues(alpha: 0.9),
    ),
    // The classic underlined-hyperlink blue, distinct from the chrome blue.
    link: const Color(0xFF0000FF),
    danger: _xpRed,
    success: const FunctionSuccess(
      primary: _xpGreen,
      secondary: Color(0xFFDFF5D0),
    ),
    warning: const FunctionWarning(
      primary: _xpYellow,
      secondary: Color(0xFFFFF3C4),
    ),
  ),
  // Self bubbles are the saturated title-bar blue; other-party bubbles are
  // the tan control face every Luna dialog box used, with a green checkmark
  // for that iconic Start-button accent.
  message: const MessagePalette(
    selfBackground: _xpBlue,
    otherBackground: _xpTan,
    selfText: Colors.white,
    otherText: _xpInk,
    selfListPrefix: Color(0xFFCFE0FB),
    otherListPrefix: _xpInkMuted,
    selfTableBorder: _xpBlueDark,
    otherTableBorder: _xpTanDark,
    selfCheckboxCheck: Colors.white,
    otherCheckboxCheck: _xpGreen,
    selfEditedLabel: Color(0xFFCFE0FB),
    otherEditedLabel: _xpInkMuted,
  ),
);
