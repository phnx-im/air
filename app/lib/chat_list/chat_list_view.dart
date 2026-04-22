// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details.dart';
import 'package:air/theme/spacings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';

import 'chat_list_content.dart';
import 'chat_list_cubit.dart';
import 'chat_list_header.dart';

class ChatListContainer extends StatelessWidget {
  const ChatListContainer({required this.isStandalone, super.key});

  final bool isStandalone;

  static Color backgroundColor(BuildContext context) {
    return CustomColorScheme.of(context).backgroundBase.secondary;
  }

  @override
  Widget build(BuildContext context) {
    final userId = context.select((UserCubit cubit) => cubit.state.userId);
    return BlocProvider(
      // Rebuild the cubit when user changes
      key: ValueKey(userId),
      create: (context) => ChatListCubit(userCubit: context.read<UserCubit>()),
      child: ChatListView(scaffold: isStandalone),
    );
  }
}

class ChatListView extends StatefulWidget {
  const ChatListView({
    super.key,
    this.scaffold = false,
    this.createChatDetailsCubit = ChatDetailsCubit.new,
  });

  final bool scaffold;
  final ChatDetailsCubitCreate createChatDetailsCubit;

  @override
  State<ChatListView> createState() => _ChatListViewState();
}

class _ChatListViewState extends State<ChatListView> {
  final _scrollController = ScrollController();

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final bgColor = ChatListContainer.backgroundColor(context);
    const fadeBleeding = Spacings.s;
    const fadeHeight = kToolbarHeight + fadeBleeding;
    // Inset the Scrollbar's track so it aligns with the list's content padding
    // and doesn't overlap the header or the fade regions.
    final scrollbarPadding = MediaQuery.paddingOf(
      context,
    ).copyWith(top: fadeHeight, bottom: fadeHeight);
    final container = Container(
      color: bgColor,
      child: MediaQuery(
        data: MediaQuery.of(context).copyWith(padding: scrollbarPadding),
        child: Scrollbar(
          controller: _scrollController,
          child: Stack(
            children: [
              Positioned.fill(
                child: ScrollConfiguration(
                  behavior: ScrollConfiguration.of(
                    context,
                  ).copyWith(scrollbars: false),
                  child: ChatListContent(
                    createChatDetailsCubit: widget.createChatDetailsCubit,
                    topPadding: fadeHeight,
                    bottomPadding: fadeHeight,
                    scrollController: _scrollController,
                  ),
                ),
              ),
              Positioned.fill(
                bottom: null,
                child: EdgeFade(
                  edge: FadeEdge.top,
                  height: fadeHeight,
                  color: bgColor,
                  curve: Curves.easeInOutQuad,
                  solidStop: 0.2,
                ),
              ),
              const Positioned.fill(bottom: null, child: ChatListHeader()),
              Positioned.fill(
                top: null,
                child: EdgeFade(
                  edge: FadeEdge.bottom,
                  height: fadeHeight,
                  color: bgColor,
                  curve: Curves.easeInOutQuad,
                ),
              ),
            ],
          ),
        ),
      ),
    );
    return widget.scaffold
        ? Scaffold(
            backgroundColor: bgColor,
            body: Stack(
              children: [
                SafeArea(bottom: false, child: container),
                const Positioned(
                  bottom: 0,
                  left: 0,
                  right: 0,
                  child: _ScrollGestureFix(),
                ),
              ],
            ),
          )
        : container;
  }
}

/// This widget fixes the issue on Android, where the swipe from the bottom
/// of the screen opens the OS app switcher and the same time scrolls the chat list
/// view.
class _ScrollGestureFix extends StatelessWidget {
  const _ScrollGestureFix();

  @override
  Widget build(BuildContext context) {
    return Container(
      height: MediaQuery.paddingOf(context).bottom,
      // Note: Color is required otherwise the scroll gesture is still handled by the widget below.
      color: Colors.transparent,
    );
  }
}
