// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/list_header/list_header_tokens.dart';
import 'package:air/features/chat_list/chat_list_content.dart';
import 'package:air/features/chat_list/chat_list_header.dart';
import 'package:flutter/material.dart';

/// Where the scrollbar track stops above the bottom edge, short of the fade so
/// the thumb stays legible against it.
const _scrollbarBottomInset = S.s64;

class ChatListView extends StatefulWidget {
  const ChatListView({
    super.key,
    this.scaffold = false,
    this.shareMode = false,
  });

  final bool scaffold;

  /// See [ChatListContent.shareMode].
  final bool shareMode;

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
    final bgColor = PanelSurface.colorOf(context);
    // On a phone the list runs behind the status bar, so the header carries
    // that inset itself and the list reserves the bar's height plus the
    // clearance below it. Read from the same device type the header's tokens
    // are, so the two densities can never disagree.
    final phone = DeviceType.isPhone;
    final safeTop = phone ? MediaQuery.paddingOf(context).top : 0.0;
    final headerHeight = safeTop + ListHeaderTokens.height;
    final container = AppScrollbar(
      // Start the track below the header rather than letting it run up behind
      // it.
      trackTop: headerHeight,
      trackBottom: _scrollbarBottomInset,
      child: ChatListContent(
        header: _Header(
          scrollOffset: _scrollOffset,
          topInset: safeTop,
          shareMode: widget.shareMode,
        ),
        headerHeight: headerHeight,
        onScrollOffset: (offset) => _scrollOffset.value = offset,
        shareMode: widget.shareMode,
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
  const _Header({
    required this.scrollOffset,
    required this.topInset,
    required this.shareMode,
  });

  final ValueNotifier<double>? scrollOffset;

  /// Status-bar inset. The header floats over a full-bleed list, so it cannot
  /// rely on a SafeArea to clear the notch.
  final double topInset;

  /// See [ChatListContent.shareMode].
  final bool shareMode;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(top: topInset),
      child: ChatListHeader(scrollOffset: scrollOffset, shareMode: shareMode),
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
