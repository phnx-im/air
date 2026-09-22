// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/nav_item/nav_item_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:flutter/widgets.dart';

/// A vertical navigation cell, a glyph above a label, for tab bars and
/// sidebars. The host supplies the [glyph] (an icon or avatar, already colored
/// for its [active] state) and owns the active pill background. NavItem owns
/// the layout, the active versus inactive label style, and the shared
/// [StateLayer] (hover ink plus press, no scale lift).
class NavItem extends StatelessWidget {
  const NavItem({
    super.key,
    required this.tokens,
    required this.glyph,
    required this.label,
    this.active = false,
    this.onTap,
    this.enabled = true,
    this.press = true,
  });

  final NavItemTokens tokens;
  final Widget glyph;
  final String label;
  final bool active;
  final VoidCallback? onTap;
  final bool enabled;

  /// Whether a tap paints the pressed state. The tab bar passes false: its
  /// feedback is the pill sliding to the tapped tab, not a press wash.
  final bool press;

  @override
  Widget build(BuildContext context) {
    return StateLayer(
      onTap: onTap,
      enabled: enabled,
      borderRadius: tokens.radius,
      surface: tokens.surface,
      press: press,
      // The active cell carries the pill background, so StateLayer suppresses
      // its hover wash and the two don't stack.
      selected: active,
      child: SizedBox(
        width: tokens.boxWidth,
        height: tokens.boxHeight,
        child: Padding(
          padding: tokens.padding,
          child: Column(
            mainAxisAlignment: .center,
            children: [
              glyph,
              SizedBox(height: tokens.labelGap),
              Text(
                label,
                maxLines: 1,
                overflow: .ellipsis,
                style: active
                    ? tokens.activeLabelStyle
                    : tokens.inactiveLabelStyle,
              ),
            ],
          ),
        ),
      ),
    );
  }
}
