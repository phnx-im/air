// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/chat_list/chat_list.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/util/resizable_panel.dart';
import 'package:provider/provider.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    const desktopLayout = HomeScreenDesktopLayout(
      chatList: ChatListContainer(isStandalone: false),
      chat: ChatScreen(),
    );
    return const ResponsiveScreen(
      mobile: ChatListContainer(isStandalone: true),
      tablet: desktopLayout,
      desktop: desktopLayout,
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
    context.read<UserSettingsCubit>().setSidebarWidth(
      userCubit: context.read(),
      value: panelWidth,
    );
  }
}
