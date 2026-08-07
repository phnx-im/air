// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Layout tokens for the frame every signed-out screen sits in, per density.
///
/// A desktop window keeps its content in a safe zone: an inset clearing the
/// window chrome (macOS traffic lights included) and a narrow column. A phone
/// has neither chrome nor width to spare, so it stays full bleed.
@immutable
class NuxScaffoldTokens {
  const NuxScaffoldTokens({
    required this.windowPadding,
    required this.contentMaxWidth,
    required this.topAlignment,
    required this.overlayInset,
  });

  /// Inset between the window edge and everything the frame holds.
  final EdgeInsets windowPadding;

  /// Width the body and footer clamp to, so fields and buttons share one edge.
  final double contentMaxWidth;

  /// Where the top slot sits along the top edge.
  final Alignment topAlignment;

  /// Inset for the floating top-left slot.
  final double overlayInset;

  /// The touch density: full bleed.
  static const NuxScaffoldTokens phone = NuxScaffoldTokens(
    windowPadding: EdgeInsets.fromLTRB(S.s16, S.s8, S.s16, S.s16),
    contentMaxWidth: double.infinity,
    topAlignment: Alignment.topLeft,
    overlayInset: S.s12,
  );

  /// The pointer density: the safe zone, and one column centered in it.
  static const NuxScaffoldTokens desktop = NuxScaffoldTokens(
    windowPadding: EdgeInsets.symmetric(horizontal: S.s96, vertical: S.s128),
    contentMaxWidth: Measure.m320,
    topAlignment: Alignment.topCenter,
    overlayInset: S.s12,
  );

  /// Keyed to the device rather than the viewport: a window narrowed to a
  /// phone's width still has chrome to clear.
  static NuxScaffoldTokens of(BuildContext context) =>
      DeviceType.isPhone ? phone : desktop;

  /// The fill every signed-out screen paints itself on.
  static Color surface(BuildContext context) =>
      SemanticPalette.of(context).backgroundBase.secondary;
}
