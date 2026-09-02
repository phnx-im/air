// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/nav_item/nav_item.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/components/nav_item/nav_item_tokens.dart';
import 'package:air/features/navigation/tab_bar_tokens.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/features/user/avatar.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Floating pill-shaped tab bar shown on mobile layouts.
///
/// A flat fill with an exposed shadow, not the frosted glass it replaced. A
/// negative [TabBarTokens.tabGap] laps the tabs over each other, and the active
/// tab is drawn frontmost, so tapping the one behind slides the front pill onto
/// it.
class AppTabBar extends StatelessWidget {
  const AppTabBar({super.key});

  @override
  Widget build(BuildContext context) {
    final activeTab = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        HomeState(:final home) => home.activeTab,
        IntroState() => HomeTab.chats,
      },
    );
    final palette = SemanticPalette.of(context);

    const tabs = HomeTab.values;
    final activeIndex = tabs.indexOf(activeTab);
    final background = palette.backgroundElevated.secondary;

    // The active tab sits on the primary pill and takes its ink, the others
    // are plain on-surface ink, like the rail.
    final navTokens = NavItemTokens(
      boxWidth: TabBarTokens.tabWidth,
      boxHeight: TabBarTokens.height,
      radius: TabBarTokens.pillRadius,
      labelGap: TabBarTokens.labelGap,
      padding: TabBarTokens.tabPadding,
      surface: background,
      activeLabelStyle: typeScale.body.mini.style(
        color: palette.accentBrand.onNavSelection,
        weight: Weight.emphasized,
      ),
      inactiveLabelStyle: typeScale.body.mini.style(
        color: palette.text.primary,
      ),
    );

    // Tabs are laid out by hand rather than in a Row: [TabBarTokens.tabGap] is
    // negative and a Row cannot take negative spacing.
    Widget tabAt(int index, HomeTab tab, {required bool active}) => Positioned(
      left: index * TabBarTokens.stride,
      top: 0,
      bottom: 0,
      width: TabBarTokens.tabWidth,
      child: _TabBarItem(tab: tab, active: active, tokens: navTokens),
    );

    return SafeArea(
      top: false,
      minimum: const EdgeInsets.only(bottom: TabBarTokens.paddingBottom),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: TabBarTokens.paddingHorizontal,
        ),
        child: Center(
          // Scale the fixed-width pill down if the screen is too narrow, so the
          // tab row can never overflow.
          child: FittedBox(
            fit: .scaleDown,
            child: Container(
              width: TabBarTokens.barWidth(tabs.length),
              height: TabBarTokens.height,
              decoration: BoxDecoration(
                color: background,
                borderRadius: BorderRadius.circular(TabBarTokens.pillRadius),
                boxShadow: Effect.elevation(Elevation.small),
              ),
              child: ClipRRect(
                borderRadius: BorderRadius.circular(TabBarTokens.pillRadius),
                // The bar floats over the tab content rather than sitting in a
                // Scaffold, so it brings its own Material for the labels to
                // inherit their text context from. Transparent: the fill above
                // is ours.
                child: Material(
                  type: .transparency,
                  child: Stack(
                    children: [
                      // Painted back-to-front: the inactive tabs, then the
                      // sliding pill, then the active tab. That ordering is what
                      // makes the selected tab the frontmost item, so it laps over
                      // its neighbour and tapping the one behind sends the pill to
                      // it.
                      for (final (index, tab) in tabs.indexed)
                        if (index != activeIndex)
                          tabAt(index, tab, active: false),
                      AnimatedPositioned(
                        duration: Effect.duration(MotionPreset.short),
                        curve: Effect.easeOutQuart,
                        // Inset on every side, so a ring of bar background shows
                        // around the pill.
                        left:
                            activeIndex * TabBarTokens.stride +
                            TabBarTokens.activePillInset,
                        top: TabBarTokens.activePillInset,
                        bottom: TabBarTokens.activePillInset,
                        width:
                            TabBarTokens.tabWidth -
                            TabBarTokens.activePillInset * 2,
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            color: palette.accentBrand.navSelection,
                            borderRadius: BorderRadius.circular(
                              TabBarTokens.pillRadius,
                            ),
                          ),
                        ),
                      ),
                      tabAt(activeIndex, activeTab, active: true),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _TabBarItem extends StatelessWidget {
  const _TabBarItem({
    required this.tab,
    required this.active,
    required this.tokens,
  });

  final HomeTab tab;
  final bool active;
  final NavItemTokens tokens;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return NavItem(
      tokens: tokens,
      active: active,
      // No pressed state, the feedback is the pill sliding to the tapped tab.
      press: false,
      label: _label(context, tab),
      onTap: () => context.read<NavigationCubit>().switchTab(tab),
      glyph: SizedBox(
        width: TabBarTokens.avatarSize,
        height: TabBarTokens.avatarSize,
        // Built under the cell, so a hovered cell hands the icon its ink.
        child: Builder(
          builder: (context) => Center(
            child: _TabIcon(
              tab: tab,
              color: active
                  ? palette.accentBrand.onNavSelection
                  : PanelSurface.inkOf(context) ?? palette.text.primary,
            ),
          ),
        ),
      ),
    );
  }

  String _label(BuildContext context, HomeTab tab) {
    final loc = AppLocalizations.of(context);
    return switch (tab) {
      HomeTab.chats => loc.homeTab_chats,
      HomeTab.profile => loc.homeTab_profile,
    };
  }
}

class _TabIcon extends StatelessWidget {
  const _TabIcon({required this.tab, required this.color});

  final HomeTab tab;
  final Color color;

  @override
  Widget build(BuildContext context) {
    switch (tab) {
      case HomeTab.chats:
        return AppIcon.messageCircle(size: TabBarTokens.iconSize, color: color);
      case HomeTab.profile:
        final profile = context.select(
          (UsersCubit cubit) => cubit.state.profile(userId: null),
        );
        return UserAvatar(profile: profile, size: TabBarTokens.avatarSize);
    }
  }
}
