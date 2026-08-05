// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the list-depth header, per density.
///
/// Geometry only: colors come from the palette at paint time, and the
/// surface the header floats over is the host's to supply.
@immutable
class ListHeaderTokens {
  const ListHeaderTokens({
    required this.paddingRight,
    required this.slotSize,
    required this.showTitle,
    required this.pillPadding,
    required this.pillMinHeight,
    required this.actionSize,
  });

  final double paddingRight;

  /// Width reserved on both sides of the title, so a centered title stays
  /// optically centered even when only the leading slot is filled. It's a
  /// floor, not a cap: a larger [actionSize] widens both slots.
  final double slotSize;

  /// Whether the header carries a title at all, the label and its pill. The
  /// two-pane layout names the list in its rail instead, so it takes an
  /// icon-only bar.
  final bool showTitle;

  final EdgeInsets pillPadding;

  /// Floor for the pill's height. The label plus [pillPadding] sets it above
  /// this.
  final double pillMinHeight;

  /// Diameter of the leading action button.
  final double actionSize;

  static const double height = S.s56;
  static const double paddingLeft = S.s20;

  /// Minimum breathing room between the title pill and the slot on either
  /// side. Doubles as the pill's width bound, so an over-long title ellipsizes
  /// instead of crowding the action.
  static const double titleGap = S.s32;

  /// Pixels of scroll over which the title pill's fill, border and shadow ramp
  /// in from transparent. At rest the list sits on the same surface as the
  /// pill, so there's nothing for it to separate from.
  static const double pillRevealDistance = S.s40;

  static const double pillRadius = CornerRadius.full;

  /// Stroke drawn outside the pill's edge. Zero drops the border outright: a
  /// zero-width [BorderSide] paints as a hairline, so it can't be passed
  /// through.
  static const double pillBorderWidth = StrokeWidth.px1;

  /// Drop shadow the pill ramps in alongside its fill. Empty by default: the
  /// pill separates off its fill and border, and a header that has to lift
  /// further off the list sets a tier here.
  static const List<BoxShadow> pillShadow = [];

  static const ListHeaderTokens phone = ListHeaderTokens(
    paddingRight: S.s16,
    slotSize: S.s40,
    showTitle: true,
    pillPadding: EdgeInsets.symmetric(horizontal: S.s20, vertical: S.s4),
    pillMinHeight: S.s40,
    actionSize: ButtonIconSize.s40,
  );

  static const ListHeaderTokens desktop = ListHeaderTokens(
    paddingRight: S.s8,
    slotSize: S.s24,
    showTitle: false,
    pillPadding: EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s2),
    pillMinHeight: S.s32,
    actionSize: ButtonIconSize.s32,
  );

  /// The two-pane layout is denser and drops the title.
  static ListHeaderTokens get current => DeviceType.isPhone ? phone : desktop;
}
