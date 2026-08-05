// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/list_group/list_group_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// A rounded card binding a run of rows into one visual unit instead of letting
/// them bleed edge to edge. The card clips its children to its radius, so the
/// first and last row pick up its corners.
class ListGroup extends StatelessWidget {
  const ListGroup({
    super.key,
    required this.tokens,
    required this.children,
    this.color,
    this.radius,
    this.padding = EdgeInsets.zero,
  });

  final ListGroupTokens tokens;
  final List<Widget> children;

  /// Card fill. Defaults to the grouped-card surface. Pass [noFill] for a group
  /// that only rounds and clips, so rows carrying their own fill keep showing
  /// the surface behind through the gaps between them.
  final Color? color;

  /// Radius override. Defaults to [ListGroupTokens.radius].
  final double? radius;

  /// Inset between the card edge and its children. None by default: a row
  /// carries its own horizontal inset, so a group that added one would double
  /// it. A card hosting something other than rows sets its own.
  final EdgeInsets padding;

  /// Fill for a group that paints none. A transparent color rather than a
  /// nullable default, so "no fill" stays tellable apart from "not passed"
  /// without a sentinel.
  static const Color noFill = Color(0x00000000);

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: color ?? SemanticPalette.of(context).backgroundBase.secondary,
        borderRadius: BorderRadius.circular(radius ?? tokens.radius),
      ),
      clipBehavior: Clip.antiAlias,
      padding: padding,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: children,
      ),
    );
  }
}
