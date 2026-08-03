// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/patterns/list_header/list_header_tokens.dart';

import 'package:air/features/chat_list/chat_list_content.dart';
import 'package:air/features/chat_list/chat_list_cubit.dart';
import 'package:air/features/chat_list/chat_list_header.dart';

/// Where the scrollbar track stops above the bottom edge, short of the fade so
/// the thumb stays legible against it.
const _scrollbarBottomInset = S.s64;

class ChatListContainer extends StatelessWidget {
  const ChatListContainer({required this.isStandalone, super.key});

  final bool isStandalone;

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
  /// The list's offset, watched by the header, which reveals its title pill as
  /// rows slide under it. Kept off `setState` so a scroll never rebuilds the
  /// list itself.
  final _scrollOffset = ValueNotifier<double>(0);

  @override
  void dispose() {
    _scrollOffset.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final bgColor = chatListBackgroundColor(context);
    // On a phone the list runs behind the status bar, so the header carries
    // that inset itself and the list reserves the bar's height plus the
    // clearance below it. Read from the same breakpoint the header's tokens
    // are, so the two densities can never disagree.
    final phone = context.breakpoint.isSmall;
    final safeTop = phone ? MediaQuery.paddingOf(context).top : 0.0;
    final headerHeight = ListHeaderTokens.of(context).height;
    final container = AppScrollbar(
      // Start the track below the header rather than letting it run up behind
      // it.
      trackTop: safeTop + headerHeight,
      trackBottom: _scrollbarBottomInset,
      child: ChatListContent(
        createChatDetailsCubit: widget.createChatDetailsCubit,
        header: _Header(scrollOffset: _scrollOffset, topInset: safeTop),
        headerHeight: headerHeight,
        onScrollOffset: (offset) => _scrollOffset.value = offset,
      ),
    );
    return widget.scaffold
        ? Scaffold(
            backgroundColor: bgColor,
            body: Stack(
              children: [
                container,
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

/// The header, rebuilt on scroll on its own so the list behind it is not.
class _Header extends StatelessWidget {
  const _Header({required this.scrollOffset, required this.topInset});

  final ValueListenable<double> scrollOffset;

  /// Status-bar inset. The header floats over a full-bleed list, so it cannot
  /// rely on a SafeArea to clear the notch.
  final double topInset;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(top: topInset),
      child: ValueListenableBuilder<double>(
        valueListenable: scrollOffset,
        builder: (context, offset, _) => ChatListHeader(scrollOffset: offset),
      ),
    );
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
