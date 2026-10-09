// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

import 'package:air/ds/foundations/foundations.dart';

/// Everything [ReminderBanner] needs to paint itself, fully resolved.
///
/// Defaults paint the warning (amber) variant. [background] and [iconColor] are
/// plain fields so a future reminder can recolour via [copyWith] without a
/// variant enum — kept minimal until a second variant actually lands.
@immutable
class ReminderBannerTokens {
  const ReminderBannerTokens({
    required this.background,
    required this.radius,
    required this.padding,
    required this.gap,
    required this.leadingColumnWidth,
    required this.iconSize,
    required this.iconColor,
    required this.titleBodyGap,
    required this.titleStyle,
    required this.bodyStyle,
    required this.dismissButtonSize,
    required this.dismissIconSize,
    required this.dismissInset,
    required this.dismissBackground,
    required this.dismissColor,
    required this.textDismissGap,
  });

  final Color background;
  final double radius;
  final EdgeInsets padding;

  /// Gap between the leading icon column and the text column.
  final double gap;

  /// Width of the leading icon column (the icon is centred within it).
  final double leadingColumnWidth;
  final double iconSize;
  final Color iconColor;

  final double titleBodyGap;
  final TextStyle titleStyle;
  final TextStyle bodyStyle;

  /// The top-right dismiss button (rendered only when the host supplies an
  /// `onDismiss`): a filled, tappable circle with a centred X glyph.
  final double dismissButtonSize;

  /// Size of the X glyph inside the dismiss button.
  final double dismissIconSize;

  /// Inset of the dismiss button from the card's top-right corner.
  final double dismissInset;

  /// Fill behind the dismiss glyph.
  final Color dismissBackground;
  final Color dismissColor;

  /// Minimum gap between the text column and the dismiss button.
  final double textDismissGap;

  factory ReminderBannerTokens.defaults(SemanticPalette colors) {
    return ReminderBannerTokens(
      background: colors.function.warning.secondary,
      radius: CornerRadius.px12,
      padding: const EdgeInsets.all(S.s12),
      gap: S.s12,
      leadingColumnWidth: S.s32,
      iconSize: S.s24,
      iconColor: colors.function.warning.primary,
      titleBodyGap: S.s2,
      titleStyle: typeScale.body.regular
          .style(color: colors.text.primary, weight: Weight.emphasized)
          .copyWith(height: 1.2),
      bodyStyle: typeScale.body.s
          .style(color: colors.text.tertiary)
          .copyWith(height: 1.2),
      dismissButtonSize: S.s24,
      dismissIconSize: S.s16,
      dismissInset: S.s4,
      dismissBackground: colors.fill.primary.withValues(alpha: 0),
      dismissColor: colors.text.primary,
      textDismissGap: S.s16,
    );
  }

  ReminderBannerTokens copyWith({
    Color? background,
    double? radius,
    EdgeInsets? padding,
    double? gap,
    double? leadingColumnWidth,
    double? iconSize,
    Color? iconColor,
    double? titleBodyGap,
    TextStyle? titleStyle,
    TextStyle? bodyStyle,
    double? dismissButtonSize,
    double? dismissIconSize,
    double? dismissInset,
    Color? dismissBackground,
    Color? dismissColor,
    double? textDismissGap,
  }) => ReminderBannerTokens(
    background: background ?? this.background,
    radius: radius ?? this.radius,
    padding: padding ?? this.padding,
    gap: gap ?? this.gap,
    leadingColumnWidth: leadingColumnWidth ?? this.leadingColumnWidth,
    iconSize: iconSize ?? this.iconSize,
    iconColor: iconColor ?? this.iconColor,
    titleBodyGap: titleBodyGap ?? this.titleBodyGap,
    titleStyle: titleStyle ?? this.titleStyle,
    bodyStyle: bodyStyle ?? this.bodyStyle,
    dismissButtonSize: dismissButtonSize ?? this.dismissButtonSize,
    dismissIconSize: dismissIconSize ?? this.dismissIconSize,
    dismissInset: dismissInset ?? this.dismissInset,
    dismissBackground: dismissBackground ?? this.dismissBackground,
    dismissColor: dismissColor ?? this.dismissColor,
    textDismissGap: textDismissGap ?? this.textDismissGap,
  );
}
