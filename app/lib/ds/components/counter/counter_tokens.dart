// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the count pill, per density.
///
/// Geometry per density, plus the pill's two color roles as palette resolvers.
/// The pill has no variants, so the roles are fixed and belong here rather than
/// at the call site.
@immutable
class CounterTokens {
  const CounterTokens({
    required this.height,
    required this.minWidth,
    required this.padding,
  });

  final double height;

  /// Floor for the pill's width, so a single-digit count still reads as a pill
  /// rather than a circle.
  final double minWidth;

  final EdgeInsets padding;

  static const double radius = CornerRadius.full;

  /// The pill inverts against the surface it sits on, so the count reads at a
  /// glance in a list of otherwise quiet rows.
  static Color fill(SemanticPalette palette) =>
      palette.function.neutral.toggleBlack;

  static Color label(SemanticPalette palette) =>
      palette.function.neutral.toggleWhite;

  static const CounterTokens phone = CounterTokens(
    height: S.s24,
    minWidth: S.s40,
    padding: EdgeInsets.symmetric(horizontal: S.s8),
  );

  static const CounterTokens desktop = CounterTokens(
    height: S.s20,
    minWidth: S.s32,
    padding: EdgeInsets.symmetric(horizontal: S.s8),
  );

  static CounterTokens get current => DeviceType.isPhone ? phone : desktop;
}
