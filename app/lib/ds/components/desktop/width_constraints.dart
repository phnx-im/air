// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';

/// A widget that constrains only the width of its child to 800 logical pixels.
/// Height remains unconstrained and is determined by the child.
class ConstrainedWidth extends StatelessWidget {
  final Widget child;
  final double width;

  const ConstrainedWidth({super.key, required this.child, this.width = 800});

  @override
  Widget build(BuildContext context) {
    return SizedBox(width: width, child: child);
  }
}
