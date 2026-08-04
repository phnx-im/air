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
    required this.minWidth,
    required this.maxWidth,
    required this.radius,
    required this.contentPadding,
    required this.margin,
    required this.titleBodyGap,
    required this.bodyActionsGap,
  });

  /// Floor for the card's width, so a one-word confirm still reads as a card
  /// rather than a tooltip.
  final double minWidth;

  /// Ceiling for the card's width, keeping body text to a readable measure on
  /// a wide window.
  final double maxWidth;

  final double radius;

  /// Inset between the card edges and its content.
  final EdgeInsets contentPadding;

  /// Inset between the card and the viewport edges, so the card never sits
  /// flush against the screen.
  final EdgeInsets margin;

  /// Gap between the title and the body.
  final double titleBodyGap;

  /// Gap between the body and the actions.
  final double bodyActionsGap;

  static const DialogTokens phone = DialogTokens(
    minWidth: S.s240,
    maxWidth: 400,
    radius: CornerRadius.px20,
    contentPadding: EdgeInsets.all(S.s24),
    margin: EdgeInsets.all(S.s24),
    titleBodyGap: S.s16,
    bodyActionsGap: S.s24,
  );

  /// The two gaps trade places: a thumb wants the actions held away from the
  /// body it just read, while a pointer lands where it aims and the title can
  /// take the extra room instead.
  static const DialogTokens desktop = DialogTokens(
    minWidth: S.s240,
    maxWidth: 400,
    radius: CornerRadius.px20,
    contentPadding: EdgeInsets.all(S.s24),
    margin: EdgeInsets.all(S.s24),
    titleBodyGap: S.s24,
    bodyActionsGap: S.s16,
  );

  static DialogTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
