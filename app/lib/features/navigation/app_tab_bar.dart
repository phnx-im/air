// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:ui';

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/tab_bar/tab_bar_tokens.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/features/user/avatar.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Floating pill-shaped tab bar shown on mobile layouts.
class AppTabBar extends StatelessWidget {
  const AppTabBar({super.key});

  @override
  Widget build(BuildContext context) {
    final activeTab = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        NavigationState_Home(:final home) => home.activeTab,
        NavigationState_Intro() => HomeTab.chats,
      },
    );
    final colors = SemanticColors.of(context);

    final pillWidth = TabBarTokens.tabWidth * HomeTab.values.length;

    return SafeArea(
      top: false,
      minimum: const EdgeInsets.only(bottom: TabBarTokens.paddingBottom),
      child: Center(
        child: DecoratedBox(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(TabBarTokens.pillRadius),
            boxShadow: Effects.elevation(Elevation.large),
          ),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(TabBarTokens.pillRadius),
            child: BackdropFilter(
              filter: ImageFilter.blur(
                sigmaX: Effects.blur(BlurLevel.medium),
                sigmaY: Effects.blur(BlurLevel.medium),
              ),
              child: Material(
                type: MaterialType.transparency,
                child: Container(
                  width: pillWidth,
                  height: TabBarTokens.height,
                  decoration: BoxDecoration(
                    color: colors.backgroundMaterial.tertiary,
                    borderRadius: BorderRadius.circular(
                      TabBarTokens.pillRadius,
                    ),
                  ),
                  child: Stack(
                    children: [
                      AnimatedPositioned(
                        duration: Effects.duration(MotionPreset.short),
                        curve: Effects.easeOutQuart,
                        top: 0,
                        bottom: 0,
                        width: TabBarTokens.tabWidth,
                        left: activeTab == HomeTab.chats
                            ? 0
                            : TabBarTokens.tabWidth,
                        child: Container(
                          decoration: BoxDecoration(
                            color: colors.fill.tertiary,
                            borderRadius: BorderRadius.circular(
                              TabBarTokens.pillRadius,
                            ),
                          ),
                        ),
                      ),
                      Row(
                        children: [
                          _TabBarItem(
                            tab: HomeTab.chats,
                            active: activeTab == HomeTab.chats,
                          ),
                          _TabBarItem(
                            tab: HomeTab.profile,
                            active: activeTab == HomeTab.profile,
                          ),
                        ],
                      ),
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
  const _TabBarItem({required this.tab, required this.active});

  final HomeTab tab;
  final bool active;

  @override
  Widget build(BuildContext context) {
    final colors = SemanticColors.of(context);
    final color = active ? colors.text.secondary : colors.text.tertiary;

    return SizedBox(
      width: TabBarTokens.tabWidth,
      height: TabBarTokens.height,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () => context.read<NavigationCubit>().switchTab(tab),
        onLongPress: tab == HomeTab.profile
            ? () => context.read<NavigationCubit>().openDeveloperSettings()
            : null,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            SizedBox(
              width: TabBarTokens.iconSize,
              height: TabBarTokens.iconSize,
              child: Center(
                child: _TabIcon(tab: tab, color: color),
              ),
            ),
            const SizedBox(height: TabBarTokens.labelGap),
            Text(
              _label(context, tab),
              style: typeScale.body.xs.style(
                color: color,
                weight: active ? Weight.emphasized : Weight.regular,
              ),
            ),
          ],
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
        return OverflowBox(
          maxWidth: TabBarTokens.avatarSize,
          maxHeight: TabBarTokens.avatarSize,
          child: UserAvatar(profile: profile, size: TabBarTokens.avatarSize),
        );
    }
  }
}
