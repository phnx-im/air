// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';

import 'chat_tile.dart';
import 'message_cubit.dart';
import 'message_list_cubit.dart';
import 'scroll_to_bottom_controller.dart';
import 'unread_divider.dart';

typedef MessageCubitCreate =
    MessageCubit Function({
      required UserCubit userCubit,
      required MessageState initialState,
    });

class MessageListView extends StatefulWidget {
  const MessageListView({
    super.key,
    this.createMessageCubit = MessageCubit.new,
    this.scrollToBottomController,
  });

  final MessageCubitCreate createMessageCubit;
  final ScrollToBottomController? scrollToBottomController;

  @override
  State<MessageListView> createState() => _MessageListViewState();
}

/// Uses a reversed [ListView] so new messages appear at the bottom (offset 0)
/// and older messages load as the user scrolls up (toward maxScrollExtent).
class _MessageListViewState extends State<MessageListView>
    with WidgetsBindingObserver {
  /// Messages that have already played their entrance animation.
  final _animatedMessages = <MessageId>{};
  final _scrollController = ScrollController();

  /// Per-message [GlobalKey]s used by [_scrollToMessage] to find a message's
  /// render object and scroll it into view.
  final _messageKeys = <MessageId, GlobalKey>{};

  /// Measured pixel heights fed to [_HeightCachingDelegate] so it can produce
  /// stable scroll-extent estimates and avoid scrollbar jitter.
  final _heightCache = <MessageId, double>{};

  /// The last message ID passed to [markAsRead], used to avoid redundant calls
  /// during rapid scroll updates.
  MessageId? _lastMarkedAsReadId;

  /// Reverse-index → MessageId lookup, rebuilt every [build] so that
  /// [_markNewestVisibleMessageAsRead] can walk it without FFI calls.
  var _reverseIndexIds = <MessageId?>[];

  /// Guards to prevent rapid-fire load requests while a load is in flight.
  bool _loadOlderPending = false;
  bool _loadNewerPending = false;

  /// The state object that last triggered a scroll-to-index action.
  /// Used to detect new scroll requests even when the index is the same.
  MessageListState? _lastScrolledState;

  /// Threshold in pixels before showing the scroll-to-bottom button.
  static const _scrollToBottomThreshold = 10.0;

  /// How close to the edge (in pixels) before triggering a page load.
  static const _loadMoreThreshold = 500.0;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _scrollController.addListener(_onScroll);
    widget.scrollToBottomController?.onScrollToBottom = _scrollToBottom;
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    widget.scrollToBottomController?.onScrollToBottom = null;
    _scrollController.removeListener(_onScroll);
    _scrollController.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _markNewestVisibleMessageAsRead();
    }
  }

  void _scrollToBottom() {
    final cubit = context.read<MessageListCubit>();
    if (cubit.state.meta.hasNewer) {
      cubit.jumpToBottom();
    } else if (_scrollController.hasClients) {
      _scrollController.animateTo(
        0,
        duration: const Duration(milliseconds: 300),
        curve: Curves.easeOut,
      );
    }
  }

  void _onScroll() {
    final cubit = context.read<MessageListCubit>();
    final state = cubit.state;
    final pos = _scrollController.position;

    // Reversed list: offset 0 = newest messages, high offset = oldest
    final isScrolledUp = pos.pixels > _scrollToBottomThreshold;
    widget.scrollToBottomController?.showButton.value =
        isScrolledUp || state.meta.hasNewer;

    // Reversed list: pixels near maxScrollExtent = oldest messages
    if (pos.pixels >= pos.maxScrollExtent - _loadMoreThreshold &&
        state.meta.hasOlder &&
        !_loadOlderPending) {
      _loadOlderPending = true;
      cubit.loadOlder();
    }
    // Reversed list: pixels near 0 = newest messages
    if (pos.pixels <= _loadMoreThreshold &&
        state.meta.hasNewer &&
        !_loadNewerPending) {
      _loadNewerPending = true;
      cubit.loadNewer();
    }

    _markNewestVisibleMessageAsRead();
  }

  /// Finds the newest message visible in the viewport and marks the
  /// conversation as read up to that point.
  ///
  /// In the reversed list, reverse index 0 (newest message) sits at scroll
  /// offset 0. We walk forward through reverse indices, accumulating cached
  /// heights, until the cumulative height exceeds the current scroll offset —
  /// that message is partially (or fully) visible at the viewport's visual
  /// bottom.
  void _markNewestVisibleMessageAsRead() {
    if (!_scrollController.hasClients) return;

    final userCubit = context.read<UserCubit>();
    if (userCubit.appState != AppState.foreground) return;

    final ids = _reverseIndexIds;
    if (ids.isEmpty) return;

    final offset = _scrollController.offset;
    double cumulative = 0;

    for (int ri = 0; ri < ids.length; ri++) {
      final id = ids[ri];
      if (id == null) continue;

      final height = _heightCache[id];
      if (height != null) {
        cumulative += height;
      }
      // Once cumulative height exceeds the scroll offset, this message
      // is at least partially visible at the visual bottom.
      if (cumulative > offset) {
        if (id == _lastMarkedAsReadId) return;
        _lastMarkedAsReadId = id;

        // Single FFI call to get the timestamp for the found message.
        final state = context.read<MessageListCubit>().state;
        final message = state.messageAt(ids.length - ri - 1);
        if (message == null) return;

        context.read<ChatDetailsCubit>().markAsRead(
          untilMessageId: message.id,
          untilTimestamp: message.timestamp,
        );
        return;
      }
    }
  }

  /// Scroll a specific message into view, even if it is outside the viewport.
  ///
  /// The lazy [ListView] only builds children near the current scroll offset,
  /// so an off-screen target's [GlobalKey] has no context. We estimate the
  /// scroll offset from cached heights, jump there to force layout, then
  /// refine with [Scrollable.ensureVisible] once the key resolves.
  void _scrollToMessage(MessageId messageId) {
    var retries = 0;
    const maxRetries = 10;

    void attempt() {
      // Bail out if the widget was unmounted or the scroll view was removed
      // while we were waiting for a post-frame callback.
      if (!mounted || !_scrollController.hasClients) return;

      // Safety cap: avoid infinite retry loops if the target never appears
      // (e.g. it was removed from the list between frames).
      if (retries++ >= maxRetries) return;

      // Check if the target's GlobalKey has a BuildContext. This is only
      // non-null when the ListView has actually built and laid out the child
      // widget — i.e. the target is inside or near the viewport.
      final key = _messageKeys[messageId];
      final ctx = key?.currentContext;
      if (ctx != null) {
        // The target is laid out — use ensureVisible for a precise,
        // animated scroll that places it at the top of the viewport.
        Scrollable.ensureVisible(
          ctx,
          alignment: 1.0,
          alignmentPolicy: ScrollPositionAlignmentPolicy.explicit,
          duration: const Duration(milliseconds: 300),
          curve: Curves.easeOut,
        );
        return;
      }

      // The target is off-screen and hasn't been built by the lazy ListView.
      // We need to move the scroll offset close enough so the ListView builds
      // it on the next layout pass.
      final state = context.read<MessageListCubit>().state;
      final targetIndex = state.messageIdIndex(messageId);
      if (targetIndex == null) return;

      // Convert to a reverse index (0 = newest, at scroll offset 0).
      // The scroll offset to reach reverse index N equals the sum of heights
      // of all items at reverse indices 0..<N (the newer messages above it).
      final count = state.loadedMessagesCount;
      final targetReverseIndex = count - targetIndex - 1;

      // Compute a fallback average height from all measured items so far.
      // On the first attempt after a Replace the cache may be empty, so we
      // fall back to 80px — a rough estimate for a single-line message.
      double avgHeight = 80;
      if (_heightCache.isNotEmpty) {
        double sum = 0;
        for (final h in _heightCache.values) {
          sum += h;
        }
        avgHeight = sum / _heightCache.length;
      }

      // Sum heights for every item between reverse index 0 (newest) and the
      // target. Use the real cached height when available, average otherwise.
      double estimated = 0;
      for (int ri = 0; ri < targetReverseIndex; ri++) {
        final msg = state.messageAt(count - ri - 1);
        estimated += (msg != null ? _heightCache[msg.id] : null) ?? avgHeight;
      }

      // Jump to the estimated offset, forcing the ListView to lay out
      // children near the target. Clamp to maxScrollExtent since our
      // estimate may overshoot.
      _scrollController.jumpTo(
        estimated.clamp(0, _scrollController.position.maxScrollExtent),
      );

      // Schedule another attempt. Each jump populates the height cache with
      // newly laid-out items, improving the estimate on the next iteration.
      // Typically converges in 1–3 frames.
      WidgetsBinding.instance.addPostFrameCallback((_) => attempt());
    }

    // Wait one frame for the current build to complete before starting.
    WidgetsBinding.instance.addPostFrameCallback((_) => attempt());
  }

  /// Scroll to the end of the reversed list (oldest message at the visual top).
  ///
  /// After a Replace the height cache is empty, so maxScrollExtent is only an
  /// estimate. Each jump forces the ListView to lay out more items near the
  /// end, improving the estimate. We repeat until the value stabilizes.
  void _jumpToEnd() {
    // Track the previous maxScrollExtent to detect convergence.
    var lastMax = -1.0;
    var retries = 0;
    const maxRetries = 10;

    void attempt() {
      // Bail out if the widget was unmounted or the scroll view was removed
      // while we were waiting for a post-frame callback.
      if (!mounted || !_scrollController.hasClients) return;

      // Safety cap: avoid infinite retry loops. In practice this converges
      // in 2–4 frames for a typical message list.
      if (retries++ >= maxRetries) return;

      // Jump to the current maxScrollExtent. On a lazy ListView this is only
      // an estimate — it depends on how many children have been laid out and
      // their measured heights vs. the average used for the rest.
      final max = _scrollController.position.maxScrollExtent;
      _scrollController.jumpTo(max);

      // Check if the estimate has stabilized. When the jump lands at the true
      // end, laying out the final children won't change the extent. A delta
      // < 1px means we've converged — no need to retry.
      if ((max - lastMax).abs() < 1.0) return;
      lastMax = max;

      // The jump forced the ListView to lay out children near the end, which
      // updates the height cache and produces a better maxScrollExtent
      // estimate. Retry on the next frame to converge further.
      WidgetsBinding.instance.addPostFrameCallback((_) => attempt());
    }

    // Wait one frame for the current build to complete before starting.
    WidgetsBinding.instance.addPostFrameCallback((_) => attempt());
  }

  @override
  Widget build(BuildContext context) {
    final state = context.select((MessageListCubit cubit) => cubit.state);
    final messageCount = state.loadedMessagesCount;

    // Reset guards on every state change — build only reruns when the cubit
    // emits a new state, so any in-flight load has completed.
    _loadOlderPending = false;
    _loadNewerPending = false;

    // Clean up stale entries for messages no longer in the window
    _messageKeys.removeWhere((id, _) => state.messageIdIndex(id) == null);
    _heightCache.removeWhere((id, _) => state.messageIdIndex(id) == null);
    _animatedMessages.removeWhere((id) => state.messageIdIndex(id) == null);

    // Build reverse-index → MessageId lookup so the delegate can read
    // live cached heights during layout (not just at build-time).
    final totalChildCount = messageCount;
    _reverseIndexIds = List<MessageId?>.filled(totalChildCount, null);
    for (int ri = 0; ri < messageCount; ri++) {
      _reverseIndexIds[ri] = state.messageAt(messageCount - ri - 1)?.id;
    }
    final reverseIndexIds = _reverseIndexIds;

    // Deferred to avoid side-effects during build.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      widget.scrollToBottomController?.showButton.value =
          (_scrollController.hasClients &&
              _scrollController.offset > _scrollToBottomThreshold) ||
          state.meta.hasNewer;
      _markNewestVisibleMessageAsRead();
    });

    final composerHeightListenable =
        widget.scrollToBottomController?.composerHeight;

    return BlocListener<MessageListCubit, MessageListState>(
      listenWhen: (prev, curr) =>
          curr.meta.scrollToIndex != null &&
          !identical(curr, _lastScrolledState),
      listener: (context, state) {
        _lastScrolledState = state;
        final scrollTo = state.meta.scrollToIndex!;
        final message = state.messageAt(scrollTo);
        if (message == null) return;

        final isNewest = scrollTo == state.loadedMessagesCount - 1;
        if (state.meta.isAtBottom && isNewest) {
          // Jump to newest (offset 0 in reversed list) — jumpToBottom case
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted && _scrollController.hasClients) {
              _scrollController.jumpTo(0);
            }
          });
        } else if (scrollTo == 0) {
          // Index 0 = oldest loaded message = visual top (maxScrollExtent)
          _jumpToEnd();
        } else {
          // Scroll a specific message into view (e.g. unread divider)
          _scrollToMessage(message.id);
        }
      },
      child: _buildList(
        composerHeightListenable,
        totalChildCount,
        reverseIndexIds,
        state,
        messageCount,
      ),
    );
  }

  Widget _buildList(
    ValueListenable<double>? composerHeightListenable,
    int totalChildCount,
    List<MessageId?> reverseIndexIds,
    MessageListState state,
    int messageCount,
  ) {
    Widget buildListView(double bottomPadding) {
      return ListView.custom(
        controller: _scrollController,
        reverse: true,
        // In a reversed list, `bottom` = visual bottom (newest messages).
        // Reserve space so the overlaid composer doesn't cover content.
        padding: EdgeInsets.only(bottom: bottomPadding),
        childrenDelegate: _HeightCachingDelegate(
          heightCache: _heightCache,
          reverseIndexIds: reverseIndexIds,
          childCount: totalChildCount,
          findChildIndexCallback: (key) {
            if (key is! ValueKey<MessageId>) return null;
            final messageId = key.value;
            final index = state.messageIdIndex(messageId);
            return index != null ? state.loadedMessagesCount - index - 1 : null;
          },
          builder: (context, reverseIndex) {
            // Convert reversed index (0 = newest) to logical index (0 = oldest)
            final index = messageCount - reverseIndex - 1;
            final message = state.messageAt(index);
            if (message == null) {
              return const SizedBox.shrink();
            }

            // Only animate a message's entrance once — mark it as seen
            final animate =
                !_animatedMessages.contains(message.id) &&
                state.isNewMessage(message.id);
            if (animate) {
              _animatedMessages.add(message.id);
            }

            final isFirstUnread =
                state.meta.firstUnreadIndex != null &&
                index == state.meta.firstUnreadIndex;

            final globalKey = _messageKeys.putIfAbsent(
              message.id,
              GlobalKey.new,
            );

            // Each message gets its own MessageCubit, keyed by ID so Flutter
            // reuses the provider when the list rebuilds with the same message.
            Widget tile = BlocProvider(
              create: (context) {
                return widget.createMessageCubit(
                  userCubit: context.read<UserCubit>(),
                  initialState: MessageState(message: message),
                );
              },
              child: ChatTile(
                isConnectionChat: state.meta.isConnectionChat ?? false,
                animated: animate,
              ),
            );

            // Insert "N unread messages" divider above the first unread message
            if (isFirstUnread) {
              final unreadCount =
                  state.loadedMessagesCount - state.meta.firstUnreadIndex!;
              tile = Column(
                children: [
                  UnreadDivider(count: unreadCount),
                  tile,
                ],
              );
            }

            // Wrap in _SizeReportingWidget to feed measured heights into
            // the cache used by _HeightCachingDelegate for scroll estimates.
            return _SizeReportingWidget(
              key: ValueKey(message.id),
              onHeightChanged: (height) {
                _heightCache[message.id] = height;
              },
              child: KeyedSubtree(key: globalKey, child: tile),
            );
          },
        ),
      );
    }

    if (composerHeightListenable == null) {
      return buildListView(0);
    }
    // Layer the list, a bottom fade gradient, and a manual scrollbar so that:
    //  - Messages fade out as they approach the composer
    //  - The scrollbar renders above the fade (not hidden behind it)
    //  - The scrollbar track stops at the top of the fade, matching the
    //    visible content area
    return ValueListenableBuilder<double>(
      valueListenable: composerHeightListenable,
      builder: (context, composerHeight, _) {
        final safeBottom = MediaQuery.of(context).padding.bottom;
        final fadeTotal = composerHeight + _fadeHeight + safeBottom;

        // Override MediaQuery padding so the Scrollbar's track ends above
        // the composer and fade zone.
        final scrollbarPadding = MediaQuery.paddingOf(
          context,
        ).copyWith(bottom: composerHeight + _fadeHeight);
        return MediaQuery(
          data: MediaQuery.of(context).copyWith(padding: scrollbarPadding),
          child: Scrollbar(
            controller: _scrollController,
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                // Disable the auto-scrollbar, we have our own above.
                ScrollConfiguration(
                  behavior: ScrollConfiguration.of(
                    context,
                  ).copyWith(scrollbars: false),
                  child: buildListView(composerHeight + _fadeHeight),
                ),
                // Gradient fade from transparent to the background color, from
                // 40px above the composer to the screen bottom.
                Positioned(
                  left: 0,
                  right: 0,
                  bottom: -safeBottom,
                  child: _BottomFade(height: fadeTotal),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

const double _fadeHeight = 40;

class _BottomFade extends StatelessWidget {
  const _BottomFade({required this.height});

  final double height;

  @override
  Widget build(BuildContext context) {
    final bgColor = CustomColorScheme.of(context).backgroundBase.primary;
    return IgnorePointer(
      child: Container(
        height: height,
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.bottomCenter,
            end: Alignment.topCenter,
            colors: [bgColor, bgColor.withValues(alpha: 0)],
            stops: const [0.2, 1.0],
          ),
        ),
      ),
    );
  }
}

/// A [SliverChildBuilderDelegate] that uses cached child heights
/// to produce stable [estimateMaxScrollOffset] values, preventing
/// scrollbar thumb jitter from variable-height items.
class _HeightCachingDelegate extends SliverChildBuilderDelegate {
  _HeightCachingDelegate({
    required NullableIndexedWidgetBuilder builder,
    required int childCount,
    required ChildIndexGetter findChildIndexCallback,
    required this.heightCache,
    required this.reverseIndexIds,
  }) : super(
         builder,
         childCount: childCount,
         findChildIndexCallback: findChildIndexCallback,
       );

  /// Live mutable cache of MessageId → measured height.
  /// Updated directly during child layout, so reads in
  /// [estimateMaxScrollOffset] see the latest values.
  final Map<MessageId, double> heightCache;

  /// Maps each reverse index to its MessageId.
  final List<MessageId?> reverseIndexIds;

  @override
  double? estimateMaxScrollOffset(
    int firstIndex,
    int lastIndex,
    double leadingScrollOffset,
    double trailingScrollOffset,
  ) {
    final total = childCount;
    if (total == null || total == 0) return null;
    // All items are laid out — exact extent known.
    if (lastIndex >= total - 1) return trailingScrollOffset;

    final laidOutCount = lastIndex - firstIndex + 1;
    if (laidOutCount <= 0) return null;

    // Use the global average from all cached heights for a stable fallback.
    // The per-viewport average (laidOutExtent / laidOutCount) fluctuates as
    // different-sized items enter/leave the viewport, causing jitter.
    final laidOutExtent = trailingScrollOffset - leadingScrollOffset;
    double avgHeight;
    if (heightCache.isNotEmpty) {
      double sum = 0;
      for (final h in heightCache.values) {
        sum += h;
      }
      avgHeight = sum / heightCache.length;
    } else {
      avgHeight = laidOutExtent / laidOutCount;
    }

    // Only estimate items AFTER the viewport. Trust trailingScrollOffset
    // as ground truth for everything before + in the viewport (matches
    // Flutter's default approach, avoids conflicting with the sliver's
    // own leadingScrollOffset).
    double afterExtent = 0;
    for (int i = lastIndex + 1; i < total; i++) {
      final id = i < reverseIndexIds.length ? reverseIndexIds[i] : null;
      afterExtent += (id != null ? heightCache[id] : null) ?? avgHeight;
    }

    return trailingScrollOffset + afterExtent;
  }
}

/// Wraps a child and reports its height after layout via [onHeightChanged].
class _SizeReportingWidget extends SingleChildRenderObjectWidget {
  const _SizeReportingWidget({
    super.key,
    required super.child,
    required this.onHeightChanged,
  });

  final ValueChanged<double> onHeightChanged;

  @override
  RenderObject createRenderObject(BuildContext context) {
    return _RenderSizeReporter(onHeightChanged);
  }

  @override
  void updateRenderObject(
    BuildContext context,
    covariant _RenderSizeReporter renderObject,
  ) {
    renderObject.onHeightChanged = onHeightChanged;
  }
}

class _RenderSizeReporter extends RenderProxyBox {
  _RenderSizeReporter(this.onHeightChanged);

  ValueChanged<double> onHeightChanged;
  double? _lastHeight;

  @override
  void performLayout() {
    super.performLayout();
    final newHeight = size.height;
    if (_lastHeight == null || (_lastHeight! - newHeight).abs() > 0.5) {
      _lastHeight = newHeight;
      // Update the cache directly during layout so that
      // estimateMaxScrollOffset (called after all children are laid out)
      // sees the latest heights in the same frame.
      onHeightChanged(newHeight);
    }
  }
}
