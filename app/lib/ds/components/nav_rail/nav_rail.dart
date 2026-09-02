// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/nav_item/nav_item.dart';
import 'package:air/ds/components/nav_item/nav_item_tokens.dart';
import 'package:air/ds/components/nav_rail/nav_rail_tokens.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// One cell of a [NavRail].
@immutable
class NavRailItem {
  const NavRailItem({required this.label, required this.glyph, this.onTap});

  final String label;

  /// Built with the foreground color of the cell's state, so an icon paints
  /// itself active or inactive without the host resolving the palette. A glyph
  /// that carries its own colors, such as an avatar, uses [active] to set
  /// itself off from the pill instead.
  final Widget Function(Color color, {required bool active}) glyph;

  final VoidCallback? onTap;
}

/// Vertical navigation rail: the desktop counterpart of the mobile tab bar.
///
/// A translucent strip on the window's left edge holding one square cell per
/// primary destination, the active one marked by a pill behind it. The rail
/// fills the height it's given and leaves its corners to the host, which clips
/// the panel group it sits in.
class NavRail extends StatelessWidget {
  const NavRail({
    super.key,
    required this.items,
    required this.activeIndex,
    this.reserveWindowControls = false,
  });

  final List<NavRailItem> items;
  final int activeIndex;

  /// Whether to reserve [NavRailTokens.windowControlsInset] above the first
  /// cell for native window controls floating over the rail's top-left. The
  /// host owns this because it's a property of the window rather than of the
  /// design: macOS draws the traffic lights there, the other desktops don't.
  final bool reserveWindowControls;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final surface = palette.roles.surfaceVariant;
    final topInset =
        NavRailTokens.paddingTop +
        (reserveWindowControls ? NavRailTokens.windowControlsInset : 0.0);

    final tokens = NavItemTokens(
      boxWidth: NavRailTokens.itemSize,
      boxHeight: NavRailTokens.itemSize,
      radius: NavRailTokens.itemRadius,
      labelGap: NavRailTokens.labelGap,
      padding: EdgeInsets.zero,
      surface: surface,
      activeLabelStyle: typeScale.body.mini.style(
        color: palette.accentBrand.onPrimary,
        weight: Weight.emphasized,
      ),
      // Inactive cells are plain on-surface ink, like Noctalia's tabs. Only
      // the active pill and the hover fill recolor them.
      inactiveLabelStyle: typeScale.body.mini.style(
        color: palette.text.primary,
      ),
    );

    return Container(
      width: NavRailTokens.width,
      color: surface,
      child: Stack(
        alignment: Alignment.topCenter,
        children: [
          // The pill snaps to the selected cell instead of sliding: the rail
          // swaps the whole pane behind it, which no amount of travel can
          // stand in for.
          Positioned(
            top: topInset + activeIndex * NavRailTokens.stride,
            width: NavRailTokens.itemSize,
            height: NavRailTokens.itemSize,
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: palette.accentBrand.primary,
                borderRadius: BorderRadius.circular(NavRailTokens.itemRadius),
              ),
            ),
          ),
          Column(
            children: [
              SizedBox(height: topInset),
              for (final (index, item) in items.indexed) ...[
                if (index > 0) const SizedBox(height: NavRailTokens.itemGap),
                NavItem(
                  tokens: tokens,
                  active: index == activeIndex,
                  label: item.label,
                  onTap: item.onTap,
                  // Built under the cell, so a hovered cell hands the glyph
                  // its ink.
                  glyph: Builder(
                    builder: (context) => item.glyph(
                      index == activeIndex
                          ? palette.accentBrand.onPrimary
                          : PanelSurface.inkOf(context) ?? palette.text.primary,
                      active: index == activeIndex,
                    ),
                  ),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}
