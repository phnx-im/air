// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Pins its child's width, leaving the height to the child.
///
/// Named for pinning rather than constraining, because that is what it does: a
/// child narrower than [width] is widened to it, which is what a centered
/// column of fields and buttons wants. A host that only wants a ceiling reaches
/// for `ConstrainedBox` instead.
///
/// [width] defaults to the widest reading measure. `double.infinity` fills
/// whatever the parent allows, which is how a surface drops the cap at compact
/// density.
class FixedWidth extends StatelessWidget {
  final Widget child;
  final double width;

  const FixedWidth({super.key, required this.child, this.width = Measure.m800});

  @override
  Widget build(BuildContext context) {
    return SizedBox(width: width, child: child);
  }
}
