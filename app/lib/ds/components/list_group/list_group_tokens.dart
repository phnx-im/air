// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for a grouped card, per density.
///
/// Geometry only: colors come from the palette at paint time, and the inset
/// between the card and its rows belongs to the rows.
@immutable
class ListGroupTokens {
  const ListGroupTokens({required this.radius});

  /// Corner radius the card clips its children to.
  final double radius;

  static const ListGroupTokens phone = ListGroupTokens(
    radius: CornerRadius.px12,
  );

  /// Same metrics as [phone] today. The density seam stays so a tighter
  /// two-pane card lands as a token change rather than a widget change.
  static const ListGroupTokens desktop = phone;

  static ListGroupTokens of(BuildContext context) =>
      context.breakpoint.isSmall ? phone : desktop;
}
