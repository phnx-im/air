// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

class AccentBrand {
  final Color primary, secondary, tertiary, quaternary;

  const AccentBrand({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class BackgroundBase {
  final Color primary, secondary, tertiary, quaternary, quinary;

  const BackgroundBase({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
    required this.quinary,
  });
}

class BackgroundElevated {
  final Color primary, secondary, tertiary, quaternary, quinary;

  const BackgroundElevated({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
    required this.quinary,
  });
}

/// Translucent fills for frosted-glass surfaces. The fill alpha is baked in,
/// so these pair directly with a sigma from `blur.dart`.
class BackgroundMaterial {
  final Color primary, secondary, tertiary, quaternary;

  const BackgroundMaterial({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class TextColors {
  final Color primary, secondary, tertiary, quaternary;

  const TextColors({
    required this.primary,
    required this.secondary,
    required this.tertiary,
    required this.quaternary,
  });
}

class SeparatorColors {
  final Color primary, secondary;

  const SeparatorColors({required this.primary, required this.secondary});
}

class FillColors {
  final Color primary, secondary, tertiary;

  const FillColors({
    required this.primary,
    required this.secondary,
    required this.tertiary,
  });
}

/// Neutrals that either stay put or flip with the theme. [white] and [black]
/// are for ink on a fixed-color surface (avatar initials, lightbox glass);
/// [toggleWhite] and [toggleBlack] swap between light and dark.
class FunctionNeutral {
  final Color white, black, toggleWhite, toggleBlack, scrim, scrimDark;

  const FunctionNeutral({
    required this.white,
    required this.black,
    required this.toggleWhite,
    required this.toggleBlack,
    required this.scrim,
    required this.scrimDark,
  });
}

class FunctionWarning {
  final Color primary, secondary;

  const FunctionWarning({required this.primary, required this.secondary});
}

class FunctionColors {
  final FunctionNeutral neutral;
  final Color link, danger, success;
  final FunctionWarning warning;

  const FunctionColors({
    required this.neutral,
    required this.link,
    required this.danger,
    required this.success,
    required this.warning,
  });
}

/// Message-bubble colors. An Air-specific extension of the design system:
/// the reference DS carries these in its message-bubble pattern tokens.
class MessageColors {
  final Color selfBackground, otherBackground;
  final Color selfText, otherText;
  final Color selfListPrefix, otherListPrefix;
  final Color selfQuoteBorder, otherQuoteBorder;
  final Color selfQuoteBackground, otherQuoteBackground;
  final Color selfTableBorder, otherTableBorder;
  final Color selfCheckboxBorder, otherCheckboxBorder;
  final Color selfCheckboxFill, otherCheckboxFill;
  final Color selfCheckboxCheck, otherCheckboxCheck;
  final Color selfEditedLabel, otherEditedLabel;

  const MessageColors({
    required this.selfBackground,
    required this.otherBackground,
    required this.selfText,
    required this.otherText,
    required this.selfListPrefix,
    required this.otherListPrefix,
    required this.selfQuoteBorder,
    required this.otherQuoteBorder,
    required this.selfQuoteBackground,
    required this.otherQuoteBackground,
    required this.selfTableBorder,
    required this.otherTableBorder,
    required this.selfCheckboxBorder,
    required this.otherCheckboxBorder,
    required this.selfCheckboxFill,
    required this.otherCheckboxFill,
    required this.selfCheckboxCheck,
    required this.otherCheckboxCheck,
    required this.selfEditedLabel,
    required this.otherEditedLabel,
  });
}
