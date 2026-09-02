// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/features/chat_list/chat_list_view.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/app_sidebar.dart';
import 'package:air/features/navigation/app_tab_bar.dart';
import 'package:air/ds/material/tab_transition.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/you/you_pane.dart';
import 'package:air/features/you/you_screen.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/components/resizable_panel/resizable_panel.dart';
import 'package:provider/provider.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) => constraints.breakpoint.isSmall
          ? const _HomeScreenMobileLayout()
          : const HomeScreenDesktopLayout(
              chatList: ChatListView(scaffold: false),
              chat: ChatScreen(),
            ),
    );
  }
}

class _HomeScreenMobileLayout extends StatelessWidget {
  const _HomeScreenMobileLayout();

  @override
  Widget build(BuildContext context) {
    final activeTab = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        HomeState(:final home) => home.activeTab,
        IntroState() => HomeTab.chats,
      },
    );

    return Stack(
      children: [
        Positioned.fill(
          child: AnimatedSwitcher(
            duration: Effect.duration(MotionPreset.regular),
            switchInCurve: Effect.easeOutQuart,
            switchOutCurve: Effect.easeOutQuart,
            transitionBuilder: tabSwitchTransition,
            child: switch (activeTab) {
              HomeTab.chats => const ChatListView(
                key: ValueKey(HomeTab.chats),
                scaffold: true,
              ),
              HomeTab.profile => const YouScreen(
                key: ValueKey(HomeTab.profile),
              ),
            },
          ),
        ),
        const Positioned(
          left: 0,
          right: 0,
          bottom: 0,
          // The boundary keeps the bar out of the route's layer, which is
          // re-recorded during scrolling; without it the whole pill repaints
          // on every scroll frame.
          child: RepaintBoundary(child: AppTabBar()),
        ),
      ],
    );
  }
}

/// Inset between the window edge and the panel group.
const _windowInset = S.s8;

/// Corner radius of the panel group holding the rail and the list.
const _groupRadius = CornerRadius.px20;

/// Two-pane layout: the navigation rail and the list panel form a rounded group
/// floating on the window, and the content pane runs full-bleed beside it.
///
/// Which panes those are follows the active tab: the chat list and the chat, or
/// the profile sections and the open section.
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
    final palette = SemanticPalette.of(context);
    final activeTab = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        HomeState(:final home) => home.activeTab,
        IntroState() => HomeTab.chats,
      },
    );
    final onChats = activeTab == HomeTab.chats;

    // Window and content pane sit on the surface, the rail and the list
    // panel on the variant, one group. It is outlined like a Noctalia panel.
    final panelSurface = palette.roles.surfaceVariant;

    return Scaffold(
      backgroundColor: palette.roles.surface,
      // Left inset only. The vertical inset belongs to the group below: put it
      // here and it also pushes the content pane down, which runs to the
      // window's top and bottom edge.
      body: Padding(
        padding: const EdgeInsets.only(left: _windowInset),
        child: ResizablePanel(
          initialWidth: context.read<UserSettingsCubit>().state.sidebarWidth,
          onResizeEnd: (width) => onResizeEnd(context, width),
          panelBuilder: (context, width) => Container(
            margin: const EdgeInsets.symmetric(vertical: _windowInset),
            clipBehavior: .antiAlias,
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(_groupRadius),
            ),
            // Drawn over the panes, otherwise they cover it and only the
            // anti-aliased corners of the outline show through.
            foregroundDecoration: BoxDecoration(
              borderRadius: BorderRadius.circular(_groupRadius),
              border: Border.all(
                color: palette.roles.outline,
                width: StrokeWidth.px1,
              ),
            ),
            // Stretched so a pane that shrink-wraps its content (a short menu,
            // say) still paints its surface over the whole group height.
            child: Row(
              crossAxisAlignment: .stretch,
              children: [
                const AppSidebar(),
                // Rail and list share a surface, so a hairline in the group's
                // outline color keeps them apart.
                ColoredBox(
                  color: palette.roles.outline,
                  child: const SizedBox(width: StrokeWidth.px1),
                ),
                PanelSurface(
                  color: panelSurface,
                  child: SizedBox(
                    width: width,
                    child: onChats ? chatList : const YouMenuPane(),
                  ),
                ),
              ],
            ),
          ),
          // The content pane has no fill of its own: it runs full-bleed on the
          // window, so what it paints on is the window color.
          content: PanelSurface(
            color: palette.roles.surface,
            child: onChats ? chat : const YouDetailPane(),
          ),
        ),
      ),
    );
  }

  void onResizeEnd(BuildContext context, double panelWidth) {
    context.read<UserSettingsCubit>().setSidebarWidth(value: panelWidth);
  }
}
