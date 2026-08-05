// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a contact request card, per density.
///
/// Geometry only: colors come from the palette and text styles from the
/// typescale at paint time.
@immutable
class ContactRequestCardTokens {
  const ContactRequestCardTokens({
    required this.containerPadding,
    required this.padding,
    required this.maxWidth,
    required this.radius,
    required this.subtitleGap,
    required this.avatarSize,
    required this.avatarPadding,
    required this.avatarLabelGap,
    required this.messageLabelGap,
    required this.actionsTopGap,
    required this.actionsGap,
  });

  /// Inset between the card and the surface it sits on.
  final EdgeInsets containerPadding;

  /// Inset inside the card, matching the dialog surface the card is shown on.
  final EdgeInsets padding;

  /// Ceiling on the card's width, so a wide surface leaves it a card instead of
  /// stretching it into a banner.
  final double maxWidth;

  final double radius;

  /// Gap between the headline and the line naming where the request came from.
  final double subtitleGap;

  final double avatarSize;

  /// Inset around the avatar block, holding it off the header above and
  /// whatever follows below.
  final EdgeInsets avatarPadding;

  /// Gap between the avatar and the prompt to uncover the picture.
  final double avatarLabelGap;

  /// Gap between the attached note's label and the note itself.
  final double messageLabelGap;

  final double actionsTopGap;
  final double actionsGap;

  static const ContactRequestCardTokens phone = ContactRequestCardTokens(
    containerPadding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s16),
    padding: EdgeInsets.all(S.s24),
    maxWidth: 400,
    radius: CornerRadius.px20,
    subtitleGap: S.s8,
    avatarSize: S.s96,
    avatarPadding: EdgeInsets.symmetric(vertical: S.s16),
    avatarLabelGap: S.s8,
    messageLabelGap: S.s4,
    actionsTopGap: S.s32,
    actionsGap: S.s12,
  );

  /// Tighter around the card than [phone]: the two-pane layout already frames
  /// the message list, so the card needs less room around it.
  static const ContactRequestCardTokens desktop = ContactRequestCardTokens(
    containerPadding: EdgeInsets.symmetric(horizontal: S.s20, vertical: S.s12),
    padding: EdgeInsets.all(S.s24),
    maxWidth: 400,
    radius: CornerRadius.px20,
    subtitleGap: S.s8,
    avatarSize: S.s96,
    avatarPadding: EdgeInsets.symmetric(vertical: S.s16),
    avatarLabelGap: S.s8,
    messageLabelGap: S.s4,
    actionsTopGap: S.s32,
    actionsGap: S.s12,
  );

  static ContactRequestCardTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
