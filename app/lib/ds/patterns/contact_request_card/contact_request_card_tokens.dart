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
  const ContactRequestCardTokens({required this.containerPadding});

  /// Inset between the card and the surface it sits on.
  final EdgeInsets containerPadding;

  /// Inset inside the card, matching the dialog surface the card is shown on.
  static const EdgeInsets padding = EdgeInsets.all(S.s24);

  /// Ceiling on the card's width, so a wide surface leaves it a card instead of
  /// stretching it into a banner.
  static const double maxWidth = Measure.m400;

  static const double radius = CornerRadius.px20;

  /// Gap between the headline and the line naming where the request came from.
  static const double subtitleGap = S.s8;

  static const double avatarSize = S.s96;

  /// Inset around the avatar block, holding it off the header above and
  /// whatever follows below.
  static const EdgeInsets avatarPadding = EdgeInsets.symmetric(vertical: S.s16);

  /// Gap between the avatar and the prompt to uncover the picture.
  static const double avatarLabelGap = S.s8;

  /// Gap between the attached note's label and the note itself.
  static const double messageLabelGap = S.s4;

  static const double actionsTopGap = S.s32;
  static const double actionsGap = S.s12;

  static const ContactRequestCardTokens phone = ContactRequestCardTokens(
    containerPadding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s16),
  );

  /// Tighter around the card than [phone]: the two-pane layout already frames
  /// the message list, so the card needs less room around it.
  static const ContactRequestCardTokens desktop = ContactRequestCardTokens(
    containerPadding: EdgeInsets.symmetric(horizontal: S.s20, vertical: S.s12),
  );

  static ContactRequestCardTokens get current =>
      DeviceType.isPhone ? phone : desktop;
}
