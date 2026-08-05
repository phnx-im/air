// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the centered dialog card, per density.
///
/// Geometry only: colors come from the palette at paint time. The scrim and
/// the enter transition belong to the host route, so these tokens cover the
/// card and the rhythm of its title, body, and actions.
@immutable
class DialogTokens {
  const DialogTokens({
    required this.titleBodyGap,
    required this.bodyActionsGap,
  });

  /// Gap between the title and the body.
  final double titleBodyGap;

  /// Gap between the body and the actions.
  final double bodyActionsGap;

  /// Floor for the card's width, so a one-word confirm still reads as a card
  /// rather than a tooltip.
  static const double minWidth = Measure.m240;

  /// Ceiling for the card's width, keeping body text to a readable measure on
  /// a wide window.
  static const double maxWidth = Measure.m400;

  static const double radius = CornerRadius.px20;

  /// Inset between the card edges and its content.
  static const EdgeInsets contentPadding = EdgeInsets.all(S.s24);

  /// Inset between the card and the viewport edges, so the card never sits
  /// flush against the screen.
  static const EdgeInsets margin = EdgeInsets.all(S.s24);

  static const DialogTokens phone = DialogTokens(
    titleBodyGap: S.s16,
    bodyActionsGap: S.s24,
  );

  /// The two gaps trade places: a thumb wants the actions held away from the
  /// body it just read, while a pointer lands where it aims and the title can
  /// take the extra room instead.
  static const DialogTokens desktop = DialogTokens(
    titleBodyGap: S.s24,
    bodyActionsGap: S.s16,
  );

  static DialogTokens get current => DeviceType.isPhone ? phone : desktop;
}
