// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/scroll/faded_scroll_frame.dart';
import 'package:air/ds/components/scroll/scroll_edges.dart';
import 'package:air/ds/patterns/chat_list/chat_list_tokens.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// The scrolling chat list: a floating header, the rows beneath it, and a fade
/// at either end where content runs past the viewport.
///
/// A pure view. Rows arrive from [itemBuilder] already sorted and resolved, so
/// the list only decides the frame around them: what the content clears at each
/// end, and which fade belongs there.
class ChatList extends StatefulWidget {
  const ChatList({
    super.key,
    required this.tokens,
    required this.backgroundColor,
    required this.header,
    required this.headerHeight,
    required this.itemCount,
    required this.itemBuilder,
    this.cacheExtent,
    this.controller,
    this.onScrollOffset,
  });

  final ChatListTokens tokens;

  /// The surface the list paints on, and the color both fades ramp from.
  final Color backgroundColor;

  /// Pinned above the list, which scrolls behind it. It floats over a
  /// full-bleed list, so the host insets it for the status bar itself.
  final Widget header;

  /// What the first row clears, before [ChatListTokens.headerClearance].
  final double headerHeight;

  final int itemCount;
  final NullableIndexedWidgetBuilder itemBuilder;

  final ScrollCacheExtent? cacheExtent;

  /// Supply one to drive a scrollbar or to scroll the list from outside.
  /// Without one the list keeps its own.
  final ScrollController? controller;

  /// Reports the offset as the list moves, for a header that reveals its title
  /// once rows slide under it. A callback rather than a rebuild, so scrolling
  /// never repaints the list itself.
  final ValueChanged<double>? onScrollOffset;

  @override
  State<ChatList> createState() => _ChatListState();
}

class _ChatListState extends State<ChatList> {
  ScrollController? _ownController;

  /// Which ends the content rests against, watched by the frame's two fades.
  final _edges = ValueNotifier<ScrollEdges>(ScrollEdges.atRest);

  ScrollController get _controller =>
      widget.controller ?? (_ownController ??= ScrollController());

  @override
  void initState() {
    super.initState();
    _controller.addListener(_onScroll);
    // A list that opens shorter than its viewport rests against both ends and
    // never scrolls, so neither fade belongs there. No scroll fires to say so.
    WidgetsBinding.instance.addPostFrameCallback((_) => _onScroll());
  }

  @override
  void didUpdateWidget(ChatList oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller == widget.controller) return;
    oldWidget.controller?.removeListener(_onScroll);
    _ownController?.removeListener(_onScroll);
    _controller.addListener(_onScroll);
  }

  @override
  void dispose() {
    widget.controller?.removeListener(_onScroll);
    _ownController?.removeListener(_onScroll);
    _ownController?.dispose();
    _edges.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (!mounted || !_controller.hasClients) return;
    _publish(_controller.position);
  }

  void _publish(ScrollMetrics metrics) {
    widget.onScrollOffset?.call(metrics.pixels);
    _edges.value = ScrollEdges.of(metrics);
  }

  @override
  Widget build(BuildContext context) {
    final tokens = widget.tokens;

    return FadedScrollFrame(
      backgroundColor: widget.backgroundColor,
      header: widget.header,
      edges: _edges,
      topFadeHeight: tokens.fades.topHeight,
      bottomFadeHeight: tokens.fades.bottomHeight,
      topSolidStop: tokens.fades.topSolidStop,
      bottomSolidStop: ChatListFadeTokens.bottomSolidStop,
      bottomOpacity: ChatListFadeTokens.bottomOpacity,
      contentTopPadding: widget.headerHeight + tokens.headerClearance,
      contentBottomPadding: tokens.contentBottomPadding,
      builder: (topPadding, bottomPadding) => ScrollConfiguration(
        // The scrollbar is the host's: it's the side that knows what chrome
        // the track has to clear at each end.
        behavior: ScrollConfiguration.of(context).copyWith(scrollbars: false),
        // Metrics notifications cover what a scroll can't: the first layout,
        // and rows arriving or leaving while the list sits still.
        child: NotificationListener<ScrollMetricsNotification>(
          onNotification: (notification) {
            _publish(notification.metrics);
            return false;
          },
          child: ListView.builder(
            controller: _controller,
            padding: EdgeInsets.only(top: topPadding, bottom: bottomPadding),
            itemCount: widget.itemCount,
            itemBuilder: widget.itemBuilder,
            scrollCacheExtent: widget.cacheExtent,
          ),
        ),
      ),
    );
  }
}
