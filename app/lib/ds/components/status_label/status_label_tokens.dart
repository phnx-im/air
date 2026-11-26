// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Tokens for the state line under a name, per host.
///
/// The glyph keeps one size everywhere. The text follows the name it sits
/// under, so each host picks its own step of the typescale.
@immutable
class StatusLabelTokens {
  const StatusLabelTokens({required this.text, this.tight = false});

  final TypeStyleToken text;

  /// Drops the text's leading, for a line whose height the host sets.
  final bool tight;

  static const double iconSize = S.s12;

  /// Between the glyph and the label.
  static const double iconGap = S.s4;

  /// The glyph and the label share one color. A status states a fact and
  /// stays quieter than the name above it.
  static Color color(SemanticPalette palette) => palette.text.tertiary;

  /// Under the name on a profile.
  static StatusLabelTokens get profile =>
      StatusLabelTokens(text: typeScale.body.s);
}
