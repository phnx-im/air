// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/features/chat_list/chat_list_view.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/responsive_screen/responsive_screen.dart';
import 'package:air/ds/foundations/color_scheme.dart';
import 'package:air/features/navigation/app_tab_bar.dart';
import 'package:air/ds/material/tab_transition.dart';
import 'package:air/ds/foundations/motion.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/you/you_screen.dart';
import 'package:air/ds/components/resizable_panel/resizable_panel.dart';
import 'package:provider/provider.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    const desktop = HomeScreenDesktopLayout(
      chatList: ChatListContainer(isStandalone: false),
      chat: ChatScreen(),
    );
    return const ResponsiveScreen(
      mobile: _HomeScreenMobileLayout(),
      tablet: desktop,
      desktop: desktop,
    );
  }
}

class _HomeScreenMobileLayout extends StatelessWidget {
  const _HomeScreenMobileLayout();

  @override
  Widget build(BuildContext context) {
    final activeTab = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        NavigationState_Home(:final home) => home.activeTab,
        NavigationState_Intro() => HomeTab.chats,
      },
    );

    return Stack(
      children: [
        Positioned.fill(
          child: AnimatedSwitcher(
            duration: motionRegular,
            switchInCurve: motionEasing,
            switchOutCurve: motionEasing,
            transitionBuilder: tabSwitchTransition,
            child: switch (activeTab) {
              HomeTab.chats => const ChatListContainer(
                key: ValueKey(HomeTab.chats),
                isStandalone: true,
              ),
              HomeTab.profile => const YouScreen(
                key: ValueKey(HomeTab.profile),
              ),
            },
          ),
        ),
        const Positioned(left: 0, right: 0, bottom: 0, child: AppTabBar()),
      ],
    );
  }
}

class HomeScreenDesktopLayout extends StatelessWidget {
  const HomeScreenDesktopLayout({
    required this.chatList,
    required this.chat,
    super.key,
  });

  final Widget chatList;
  final Widget chat;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: CustomColorScheme.of(context).backgroundBase.primary,
      body: Row(
        children: [
          ResizablePanel(
            initialWidth: context.read<UserSettingsCubit>().state.sidebarWidth,
            onResizeEnd: (width) => onResizeEnd(context, width),
            child: chatList,
          ),
          Expanded(child: chat),
        ],
      ),
    );
  }

  void onResizeEnd(BuildContext context, double panelWidth) {
    context.read<UserSettingsCubit>().setSidebarWidth(value: panelWidth);
  }
}
