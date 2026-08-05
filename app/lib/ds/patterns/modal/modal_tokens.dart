// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the modal surface, per density.
///
/// Geometry only: colors come from the palette at paint time. The scrim
/// and the barrier belong to the route, so nothing here describes them.
@immutable
class ModalShellTokens {
  const ModalShellTokens({
    required this.minWidth,
    required this.maxWidth,
    required this.minHeight,
    required this.maxHeight,
    required this.cardRadius,
    required this.containerPadding,
    required this.contentPaddingLeft,
    required this.contentPaddingRight,
  });

  /// Width envelope of the card. The card hugs its content between the two,
  /// so `minWidth` is what keeps a short form from collapsing to its label.
  final double minWidth;
  final double maxWidth;

  /// Height envelope of the card, same deal as the width.
  final double minHeight;
  final double maxHeight;

  final double cardRadius;

  /// Inset between the viewport edge and the card, so the scrim stays visible
  /// all the way around it.
  final EdgeInsets containerPadding;

  /// Inset for the content the host puts below the header. The header owns its
  /// own padding, so this doesn't apply to it.
  final double contentPaddingLeft;
  final double contentPaddingRight;

  /// Scale the card enters from, growing to 1. Static rather than per-density,
  /// as only the desktop presentation animates in as a card. Pair it with
  /// `Effect.duration(MotionPreset.regular)` and [Effect.easeOutQuart] at the
  /// call site.
  static const double entryScaleBegin = 0.9;

  /// Scale the card leaves for. Shallower than [entryScaleBegin] and over the
  /// shorter preset, so dismissing reads as the card stepping back rather than
  /// as its entrance played backwards.
  static const double exitScaleEnd = 0.92;

  /// A full-screen modal ignores the envelope: it fills the viewport instead of
  /// clamping to a card. The values are neutral, so the clamp stays a no-op
  /// wherever one is applied anyway.
  static const ModalShellTokens phone = ModalShellTokens(
    minWidth: 0,
    maxWidth: double.infinity,
    minHeight: 0,
    maxHeight: double.infinity,
    cardRadius: CornerRadius.px0,
    containerPadding: EdgeInsets.zero,
    contentPaddingLeft: S.s16,
    contentPaddingRight: S.s16,
  );

  static const ModalShellTokens desktop = ModalShellTokens(
    minWidth: 400,
    maxWidth: 480,
    minHeight: 240,
    maxHeight: 600,
    cardRadius: CornerRadius.px24,
    containerPadding: EdgeInsets.all(S.s24),
    contentPaddingLeft: S.s16,
    contentPaddingRight: S.s16,
  );

  /// The two-pane layout has room beside the modal and shows a card, a phone
  /// gives it the whole screen.
  static ModalShellTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;

  /// The fill the modal paints itself on: the base surface where it takes over
  /// the screen, the elevated one where it floats as a card.
  ///
  /// A shared resolver rather than a field, because the header has to occlude
  /// content scrolling under it with exactly the fill the shell paints.
  /// Deriving both from the context that also picks the token set is what keeps
  /// the two from drifting.
  static Color surface(BuildContext context) => context.breakpoint.isSmall
      ? SemanticPalette.of(context).backgroundBase.primary
      : SemanticPalette.of(context).backgroundElevated.primary;
}

/// Layout tokens for the modal's chrome row, per density.
///
/// Geometry only: the fill behind the row comes from
/// [ModalShellTokens.surface], the glyph and label colors from the palette.
@immutable
class DialogHeaderTokens {
  const DialogHeaderTokens({
    required this.height,
    required this.contentPadding,
    required this.slotWidth,
    required this.actionSize,
  });

  final double height;

  /// Inset between the slots and the row's edges.
  final EdgeInsets contentPadding;

  /// Width reserved on **both** sides of the title, so a centered title stays
  /// optically centered even when only one slot is filled. It's a floor, not a
  /// cap: a larger [actionSize] widens both slots.
  final double slotWidth;

  /// Diameter of the leading and trailing action buttons.
  final double actionSize;

  // 56 on both densities: every header in the app shares the chat header's
  // height, so a button centres at the same y wherever it appears.
  static const DialogHeaderTokens phone = DialogHeaderTokens(
    height: S.s56,
    contentPadding: EdgeInsets.symmetric(horizontal: S.s12),
    slotWidth: S.s40,
    actionSize: ButtonIconSize.s40,
  );

  static const DialogHeaderTokens desktop = DialogHeaderTokens(
    height: S.s56,
    contentPadding: EdgeInsets.symmetric(horizontal: S.s12),
    slotWidth: S.s40,
    actionSize: ButtonIconSize.s24,
  );

  /// The full-screen modal takes the touch density, the card the denser one.
  static DialogHeaderTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
