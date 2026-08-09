// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/nav_rail/nav_rail.dart';
import 'package:air/ds/components/nav_rail/nav_rail_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/avatar.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// The primary navigation of the two-pane layout: a [NavRail] on the window's
/// left edge, holding the same destinations the mobile tab bar shows.
class AppSidebar extends StatelessWidget {
  const AppSidebar({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final activeTab = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        HomeState(:final home) => home.activeTab,
        IntroState() => HomeTab.chats,
      },
    );

    const tabs = HomeTab.values;

    return NavRail(
      activeIndex: tabs.indexOf(activeTab),
      // The rail holds the window's top-left corner, where the traffic lights
      // land.
      reserveWindowControls: Chrome.windowControlsFloat,
      items: [
        for (final tab in tabs)
          NavRailItem(
            label: switch (tab) {
              HomeTab.chats => loc.homeTab_chats,
              HomeTab.profile => loc.homeTab_profile,
            },
            onTap: () => context.read<NavigationCubit>().switchTab(tab),
            glyph: (color) => switch (tab) {
              HomeTab.chats => AppIcon.messageCircle(
                size: NavRailTokens.iconSize,
                color: color,
              ),
              HomeTab.profile => const _ProfileAvatar(),
            },
          ),
      ],
    );
  }
}

class _ProfileAvatar extends StatelessWidget {
  const _ProfileAvatar();

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: null),
    );
    return UserAvatar(profile: profile, size: NavRailTokens.avatarSize);
  }
}
