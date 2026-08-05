// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:math';

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/anchored_list/anchored_list.dart';
import 'package:air/util/anchored_list/controller.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/components/scroll/edge_fade.dart';
import 'package:air/ds/components/scroll/scroll_fade_tokens.dart';

import 'package:air/features/message_list/message_row_container.dart';
import 'package:air/features/message_list/date_divider.dart';
import 'package:air/features/message_list/floating_date_header.dart';
import 'package:air/features/message_list/jump_highlight.dart';
import 'package:air/features/message_list/message_cubit.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/message_list/message_reactions.dart';
import 'package:air/features/message_list/scroll_to_bottom_controller.dart';
import 'package:air/features/message_list/time_reveal.dart';
import 'package:air/features/message_list/unread_divider.dart';

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
    this.headerScrollOffset,
  });

  final MessageCubitCreate createMessageCubit;
  final ScrollToBottomController? scrollToBottomController;

  /// Reports how much content sits scrolled under the top of the viewport, 0 at
  /// the top of the loaded history. Drives the chat header pill's reveal.
  final ValueNotifier<double>? headerScrollOffset;

  /// The surface the list paints on: the window in the two-pane layout, where
  /// the chat is the content pane, or its own background tier when it fills the
  /// screen. The fades at both edges blend into it, so it has to be the color
  /// the user actually sees.
  static Color backgroundColor(BuildContext context) =>
      PanelSurface.maybeOf(context) ??
      SemanticPalette.of(context).backgroundBase.primary;

  @override
  State<MessageListView> createState() => _MessageListViewState();
}

/// Integrates [AnchoredList] with [MessageListCubit] to display a paginated,
/// scroll-stable chat message list.
///
/// Responsibilities beyond rendering:
///  - Drives the scroll-to-bottom FAB via [ScrollToBottomController].
///  - Marks the conversation as read up to the newest visible message.
///  - Routes cubit scroll-to-index commands to the [AnchoredListController].
class _MessageListViewState extends State<MessageListView>
    with WidgetsBindingObserver {
  /// Messages eligible for an entrance animation. Admitted at arrival time
  /// when the user was visually at the bottom, then evicted after
  /// [_animationWindow] so the set stays bounded and a tile that remounts
  /// later (e.g. after scroll-out-and-back) no longer replays the animation.
  final _animatingMessages = <MessageId>{};

  final _listController = AnchoredListController();

  /// The last message ID passed to [markAsRead], used to avoid redundant calls
  /// during rapid scroll updates.
  MessageId? _lastMarkedAsReadId;

  MessageListCubit? _commandsCubit;
  StreamSubscription<MessageListCommand>? _commandSubscription;
  StreamSubscription<Set<MessageId>>? _incomingMessagesSubscription;
  StreamSubscription<JumpedToEvent>? _jumpedToIdSubscription;
  bool _initialUnreadScrollHandled = false;

  /// When there are unread messages, we automatically scroll to the first
  /// unread message. During that automatic scroll, we should not mark messages
  /// as read.
  bool _awaitingInitialUnreadScroll = false;

  /// Whether the user is currently scrolling (or has scrolled within the
  /// last [_floatingHeaderHideDelay]). Drives the floating date header's
  /// fade-in/out so the pill stays out of the way when the user isn't
  /// actively orienting in the timeline.
  ///
  /// We use a [ValueNotifier] (rather than [State.setState]) so the change
  /// only rebuilds the header itself, not the surrounding tree.
  final ValueNotifier<bool> _scrollActive = ValueNotifier<bool>(false);
  Timer? _floatingHeaderHideTimer;

  /// Distance scrolled away from the newest message, which reveals the fade at
  /// the composer's edge. The list is reversed, so this is the raw offset.
  final ValueNotifier<double> _bottomFadeOffset = ValueNotifier<double>(0);

  /// Number of pixels we dragged down the message list since the beginning of
  /// the drag.
  double _downwardDragSinceStart = 0;
  bool _keyboardDismissedThisDrag = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    widget.scrollToBottomController?.onScrollToBottom = _scrollToBottom;

    // Drive the scroll-to-bottom button from the viewport and hasNewer.
    _listController.isAtBottom.addListener(_updateShowButton);
    _listController.newestVisibleId.addListener(_updateShowButton);
    _listController.newestVisibleId.addListener(
      _markCurrentVisibleMessageAsRead,
    );
    // We listen to events to know when the initial scroll to the first unread
    // message is complete.
    _jumpedToIdSubscription = _listController.jumpedToId
        .where((event) => event.intent == JumpIntent.firstUnread)
        .listen((_) => _onInitialUnreadScrollSettled());
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Idempotent and cheap when already warmed.
    warmUpReactionEmojis(context);
    final cubit = context.read<MessageListCubit>();
    if (identical(cubit, _commandsCubit)) return;
    _commandSubscription?.cancel();
    _incomingMessagesSubscription?.cancel();
    _commandsCubit = cubit;
    _commandSubscription = cubit.commands.listen(_handleCommand);
    _incomingMessagesSubscription = cubit.incomingMessages.listen(
      _admitIncomingAnimations,
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    widget.scrollToBottomController?.onScrollToBottom = null;
    _commandSubscription?.cancel();
    _incomingMessagesSubscription?.cancel();
    _jumpedToIdSubscription?.cancel();
    _listController.isAtBottom.removeListener(_updateShowButton);
    _listController.newestVisibleId.removeListener(_updateShowButton);
    _listController.newestVisibleId.removeListener(
      _markCurrentVisibleMessageAsRead,
    );
    _listController.dispose();
    _floatingHeaderHideTimer?.cancel();
    _scrollActive.dispose();
    _bottomFadeOffset.dispose();
    super.dispose();
  }

  /// Admits freshly-arrived messages to the entrance-animation set iff the
  /// user is visually at the bottom right now. Messages that don't qualify
  /// are not tracked at all — no exclusion bookkeeping needed.
  ///
  /// Evicts ids after [_animationWindow]: by then the animation has either
  /// played or the tile was never built, and either way further tracking
  /// would only grow the set for the lifetime of the view.
  void _admitIncomingAnimations(Set<MessageId> ids) {
    if (!_listController.isAtBottom.value) return;
    _animatingMessages.addAll(ids);
    Future.delayed(_animationWindow, () {
      _animatingMessages.removeAll(ids);
    });
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      // Schedule after a microtask so the AppState stream update reaches
      // UserCubit before we check userCubit.appState.
      Future.microtask(_markCurrentVisibleMessageAsRead);
    }
  }

  /// Scrolls to the newest message. Two cases:
  ///  - If newer pages exist beyond the loaded window, ask the cubit to
  ///    reload from the bottom (jumpToBottom replaces the data).
  ///  - Otherwise, smoothly animate within the current data.
  void _scrollToBottom() {
    final cubit = context.read<MessageListCubit>();
    if (cubit.state.hasNewer) {
      cubit.jumpToBottom();
    } else {
      _listController.scrollToBottom();
    }
  }

  /// Shows the scroll-to-bottom button when the newest message is out of
  /// view or when there are newer messages not yet loaded.
  void _updateShowButton() {
    final state = context.read<MessageListCubit>().state;
    final controller = widget.scrollToBottomController;
    controller?.showButton.value =
        state.hasNewer || !_isNewestMessageVisible(state);
  }

  /// Whether the newest loaded message is on screen.
  ///
  /// We go by the newest visible row rather than the scroll offset alone: the
  /// initial jump to the first unread message parks the list a little above
  /// the bottom, with the newest row still in view and already marked as read,
  /// and a purely offset-based check would leave the button up with nothing
  /// left to scroll to.
  bool _isNewestMessageVisible(MessageListStateWrapper state) {
    if (_listController.isAtBottom.value) return true;
    final data = state.messageData;
    if (data.length == 0) return true;
    return _listController.currentNewestVisibleId == data[0].id;
  }

  /// Marks the conversation as read up to the newest message currently visible
  /// in the viewport.
  ///
  /// The anchored list computes actual visible items from its measured layout
  /// and exposes the newest visible ID via its controller, so this avoids the
  /// old fixed-height approximation based on scroll offset alone.
  void _markCurrentVisibleMessageAsRead() {
    // We skip marking messages as read during the automatic initial scroll.
    if (_awaitingInitialUnreadScroll) return;

    // AnchoredList tracks the newest item currently visible in the viewport
    // and exposes its ID via the controller. Use that directly instead of
    // approximating visibility from scroll offset and guessed item heights.
    final visibleId = _listController.currentNewestVisibleId;
    if (visibleId is! MessageId) return;

    final userCubit = context.read<UserCubit>();
    if (userCubit.appState != AppState.foreground) return;

    // Skip duplicate mark-as-read calls while the same message remains the
    // newest visible item during incremental scroll updates.
    if (visibleId == _lastMarkedAsReadId) return;

    final state = context.read<MessageListCubit>().state;
    final message = state.messageById(visibleId);
    if (message == null) return;

    _lastMarkedAsReadId = message.id;
    context.read<ChatDetailsCubit>().markAsRead(
      untilMessageId: message.id,
      untilTimestamp: message.timestamp,
    );
  }

  /// We lift the mark-as-read suppression once the initial scroll to the first
  /// unread message.
  void _onInitialUnreadScrollSettled() {
    if (!_awaitingInitialUnreadScroll || !mounted) return;
    setState(() {
      _awaitingInitialUnreadScroll = false;
    });
    _markCurrentVisibleMessageAsRead();
  }

  // The list is reversed, so pixels count from the bottom and what sits under
  // the header is maxScrollExtent - pixels.
  void _updateScrollOffsets(ScrollMetrics metrics) {
    widget.headerScrollOffset?.value = max(
      0.0,
      metrics.maxScrollExtent - metrics.pixels,
    );
    _bottomFadeOffset.value = max(0.0, metrics.pixels);
  }

  /// Shows the floating header during active scroll and hides it again
  /// after [_floatingHeaderHideDelay] of inactivity.
  bool _handleScrollNotification(ScrollNotification notification) {
    _updateScrollOffsets(notification.metrics);
    if (notification is ScrollStartNotification) {
      _downwardDragSinceStart = 0;
      _keyboardDismissedThisDrag = false;
      // This is a user-initiated scroll.
      if (notification.dragDetails != null) {
        _onInitialUnreadScrollSettled();
      }
    } else if (notification is ScrollUpdateNotification) {
      _floatingHeaderHideTimer?.cancel();
      _scrollActive.value = true;
      _maybeDismissKeyboardOnDrag(notification);
    } else if (notification is ScrollEndNotification) {
      _floatingHeaderHideTimer?.cancel();
      _floatingHeaderHideTimer = Timer(_floatingHeaderHideDelay, () {
        if (!mounted) return;
        _scrollActive.value = false;
      });
    }
    return false;
  }

  /// Dismisses the keyboard once a drag has pulled down past
  /// [_keyboardDismissDragThreshold].
  void _maybeDismissKeyboardOnDrag(ScrollUpdateNotification notification) {
    if (!DeviceType.isPhone || _keyboardDismissedThisDrag) return;
    final drag = notification.dragDetails;
    if (drag == null) return;
    _downwardDragSinceStart = max(0, _downwardDragSinceStart + drag.delta.dy);
    if (_downwardDragSinceStart < _keyboardDismissDragThreshold) return;
    _keyboardDismissedThisDrag = true;
    FocusScope.of(context).unfocus();
  }

  /// Resolves the timestamp of the message with [id], for the floating
  /// header. Returns null if the id is unknown or the loaded data has been
  /// trimmed past it.
  DateTime? _resolveMessageTimestamp(Object id) {
    if (id is! MessageId) return null;
    final state = context.read<MessageListCubit>().state;
    return state.messageById(id)?.timestamp;
  }

  void _handleCommand(MessageListCommand command) {
    switch (command) {
      case MessageListCommand_ScrollToBottom():
        _listController.scrollToBottom(duration: Duration.zero);
      case MessageListCommand_ScrollToId(:final messageId):
        _listController.goToId(messageId, intent: JumpIntent.quotedMessage);
    }
  }

  void _scheduleInitialUnreadScroll(MessageListStateWrapper state) {
    if (_initialUnreadScrollHandled || state.firstUnreadIndex == null) {
      return;
    }
    final message = state.messageAt(state.firstUnreadIndex!);
    if (message == null) return;

    _initialUnreadScrollHandled = true;
    _awaitingInitialUnreadScroll = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        _listController.goToId(message.id, intent: JumpIntent.firstUnread);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = context.select((MessageListCubit cubit) => cubit.state);

    // Deferred to avoid side-effects during build.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _updateShowButton();
    });
    _scheduleInitialUnreadScroll(state);

    final composerHeightListenable =
        widget.scrollToBottomController?.composerHeight;

    return JumpHighlightScope(
      jumpedToId: _listController.jumpedToId,
      child: _buildList(composerHeightListenable, state),
    );
  }

  /// Builds the [AnchoredList], wiring pagination and jump-to-message
  /// callbacks to the cubit.
  ///
  /// When a [composerHeightListenable] is provided, the list's bottom
  /// padding tracks the composer height so content isn't hidden behind it.
  Widget _buildList(
    ValueListenable<double>? composerHeightListenable,
    MessageListStateWrapper state,
  ) {
    // Height of safe area + tool bar
    final mediaPadding = MediaQuery.paddingOf(context);
    final fades = MessageListFadeTokens.of(context);
    // Height of the safe area above the toolbar. The screen extends its body
    // behind the app bar, so the bar's own height is part of this padding.
    final statusBarHeight = max(mediaPadding.top - kToolbarHeight, 0.0);
    // Total height of the top fade: the status bar and the header bar it has to
    // cover, plus the ramp trailing below them.
    final fadeHeight = statusBarHeight + kToolbarHeight + fades.topTail;
    // Y-coordinate where content comes clear of the fade. Used as the list's
    // top inset so rows at rest, jumps and the unread divider all land below
    // it.
    final topInset = fadeHeight + MessageListFadeTokens.contentTopGap;
    // Y-coordinate of the floating date pill's top edge. Sits just below
    // the safe area, in the toolbar zone — the inline divider is hidden
    // when its pill reaches this slot, making the swap visually in-place.
    final pillTop = mediaPadding.top + S.s16;
    // Item-top y-coordinate at or above which the inline-to-floating pill swap
    // fires. The inline divider's pill sits [S.s32] below the divider's
    // top, so when an item's top reaches this y, its pill aligns with
    // [pillTop]. Both the inline-pill hide and the floating-pill show gate on
    // this threshold so they stay in sync.
    final swapTopThreshold = pillTop - S.s32;
    // Solid color for the safe area
    final bgColor = MessageListView.backgroundColor(context);
    final newestOwnIndex = _newestOwnIndex(state);

    Widget buildAnchoredList({double bottomPadding = 0.0}) {
      // Metrics notifications cover the initial layout and content growth,
      // where no scroll activity fires: the pill has to already be revealed
      // when a chat opens at the bottom of its history.
      Widget list = NotificationListener<ScrollMetricsNotification>(
        onNotification: (notification) {
          _updateScrollOffsets(notification.metrics);
          return false;
        },
        child: NotificationListener<ScrollNotification>(
          onNotification: _handleScrollNotification,
          child: AnchoredList<UiChatMessage>(
            data: context.read<MessageListCubit>().messageData,
            controller: _listController,
            idExtractor: (msg) => msg.id,
            topPadding: topInset,
            bottomPadding: bottomPadding,
            oldestVisibleTopThreshold: swapTopThreshold,
            canLoadOlder: state.hasOlder,
            // We should not load newer messages when we scroll to the first
            // unread message.
            canLoadNewer: state.hasNewer && !_awaitingInitialUnreadScroll,
            onLoadOlder: () {
              context.read<MessageListCubit>().loadOlder();
            },
            onLoadNewer: () {
              context.read<MessageListCubit>().loadNewer();
            },
            onLoadAround: (id) async {
              if (id is MessageId) {
                await context.read<MessageListCubit>().jumpToMessage(
                  messageId: id,
                );
              }
            },
            itemBuilder: (context, message, index) {
              return _buildMessageTile(state, message, index, newestOwnIndex);
            },
          ),
        ),
      );

      // On mobile, we want to dismiss the keyboard by tapping anywhere in the
      // list, except when tapping interactive elements like e.g. links.
      if (DeviceType.isPhone) {
        list = GestureDetector(
          behavior: .translucent,
          onTap: () => FocusScope.of(context).unfocus(),
          child: list,
        );
        // With no pointer to hover a row with, a leftward drag is how the times
        // the rows keep to themselves are reached.
        list = TimeRevealScope(child: list);
      }
      return list;
    }

    final floatingHeader = Positioned(
      top: pillTop,
      left: 0,
      right: 0,
      child: Center(
        child: FloatingDateHeader(
          oldestVisibleId: _listController.oldestVisibleId,
          isOldestVisibleHoisted: _listController.isOldestVisibleHoisted,
          resolveTimestamp: _resolveMessageTimestamp,
          scrollActive: _scrollActive,
        ),
      ),
    );

    // Header gradient, covering the status bar and the header bar and bleeding
    // into the list. It ramps from the very top rather than sitting on a solid
    // block, so rows sliding under the bar stay faintly visible behind it.
    final headerFade = Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: EdgeFade(
        edge: FadeEdge.top,
        height: fadeHeight,
        color: bgColor,
        curve: Curves.easeInOutQuad,
        opacity: fades.topOpacity,
      ),
    );
    // Fade at the composer's edge. Fixed height, so it is independent of how
    // far the composer has grown, and revealed by scroll: at rest on the newest
    // message there is nothing below the fold to fade.
    final bottomFade = Positioned.fill(
      top: null,
      child: ValueListenableBuilder<double>(
        valueListenable: _bottomFadeOffset,
        // The reveal scales the gradient itself rather than wrapping it in an
        // Opacity, which would cost a save layer for every frame of the scroll.
        builder: (context, offset, _) {
          final reveal = (offset / MessageListFadeTokens.bottomRevealDistance)
              .clamp(0.0, 1.0);
          return EdgeFade(
            edge: FadeEdge.bottom,
            height: fades.bottomHeight,
            color: bgColor,
            curve: Curves.easeInOutQuad,
            opacity: fades.bottomOpacity * reveal,
          );
        },
      ),
    );

    if (composerHeightListenable == null) {
      return Stack(
        clipBehavior: .none,
        children: [buildAnchoredList(), bottomFade, headerFade, floatingHeader],
      );
    }
    // Layer the list, a bottom fade gradient, and a manual scrollbar so that:
    //  - Messages fade out as they approach the composer
    //  - The scrollbar renders above the fade (not hidden behind it)
    //  - The scrollbar track stops at the top of the fade, matching the
    //    visible content area
    return ValueListenableBuilder<double>(
      valueListenable: composerHeightListenable,
      builder: (context, composerHeight, _) {
        final bottomInset = max(mediaPadding.bottom, S.s12);
        final listBottomPadding = composerHeight + bottomInset + _bottomGap;

        return AppScrollbar(
          // Keep the track clear of the safe area at the top and of the
          // composer and its fade zone at the bottom, so it spans the content
          // the user can actually see.
          trackTop: mediaPadding.top,
          trackBottom: listBottomPadding,
          child: Stack(
            clipBehavior: .none,
            children: [
              // Disable the auto-scrollbar, we have our own above.
              ScrollConfiguration(
                behavior: ScrollConfiguration.of(
                  context,
                ).copyWith(scrollbars: false),
                child: buildAnchoredList(bottomPadding: listBottomPadding),
              ),
              bottomFade,
              headerFade,
              floatingHeader,
            ],
          ),
        );
      },
    );
  }

  /// Index of the newest message the user sent, or -1 where the chat has none
  /// to point at.
  ///
  /// Withheld while newer rows remain unloaded: the newest own row in the
  /// window is not necessarily the newest one there is.
  int _newestOwnIndex(MessageListStateWrapper state) {
    if (state.hasNewer) return -1;
    final ownUserId = context.read<UserCubit>().state.userId;
    final data = state.messageData;
    for (var i = 0; i < data.length; i++) {
      final message = data[i].message;
      if (message is! UiMessage_Content) continue;
      if (message.field0.sender == ownUserId) return i;
    }
    return -1;
  }

  /// Builds a single message row, optionally preceded by date and unread
  /// dividers (in that visual order, top-to-bottom).
  Widget _buildMessageTile(
    MessageListStateWrapper state,
    UiChatMessage message,
    int index,
    int newestOwnIndex,
  ) {
    final animated = _animatingMessages.contains(message.id);

    final isFirstUnread =
        state.firstUnreadIndex != null &&
        state.messageAt(state.firstUnreadIndex!)?.id == message.id;

    final showDateDivider = _shouldShowDateDivider(state, message, index);

    final userCubit = context.read<UserCubit>();
    // The end of the chat, where the conversation shows its time. Index 0 is
    // the newest loaded row, which is the newest there is only once nothing
    // newer remains to load.
    final isNewest = index == 0 && !state.hasNewer;

    Widget tile = _MessageTileCubitHost(
      key: ValueKey(message.id),
      userCubit: userCubit,
      message: message,
      createMessageCubit: widget.createMessageCubit,
      child: MessageRowContainer(
        isConnectionChat: state.isConnectionChat ?? false,
        animated: animated,
        isNewest: isNewest,
        isNewestOwn: index == newestOwnIndex,
      ),
    );

    if (!showDateDivider && !isFirstUnread) return tile;

    final unreadCount = isFirstUnread ? state.unreadCount : 0;
    // The inline [DateDivider] is hidden (but keeps its layout space) only
    // once its pill has actually risen to the floating pill's slot — i.e.
    // when this message is the oldest visible *and* the controller reports
    // it as hoisted. The floating header gates on the same boolean so the
    // swap is symmetric.
    final inlineDivider = showDateDivider
        ? AnimatedBuilder(
            animation: Listenable.merge([
              _listController.oldestVisibleId,
              _listController.isOldestVisibleHoisted,
            ]),
            builder: (context, child) {
              final isOldest =
                  _listController.currentOldestVisibleId == message.id;
              final hoisted = _listController.isOldestVisibleHoisted.value;
              return Visibility(
                visible: !(isOldest && hoisted),
                maintainSize: true,
                maintainAnimation: true,
                maintainState: true,
                child: child!,
              );
            },
            child: DateDivider(date: message.timestamp),
          )
        : null;
    return Column(
      children: [
        ?inlineDivider,
        if (isFirstUnread) UnreadDivider(count: unreadCount),
        tile,
      ],
    );
  }

  /// True when [message] is the first message of its local day visible in the
  /// list. We render a divider above the topmost loaded message only when no
  /// older messages remain to load.
  bool _shouldShowDateDivider(
    MessageListStateWrapper state,
    UiChatMessage message,
    int index,
  ) {
    final olderIndex = index + 1;
    if (olderIndex >= state.messageData.length) {
      return !state.hasOlder;
    }
    final older = state.messageData[olderIndex];
    return !_isSameLocalDay(older.timestamp, message.timestamp);
  }
}

bool _isSameLocalDay(DateTime a, DateTime b) {
  final aLocal = a.toLocal();
  final bLocal = b.toLocal();
  return aLocal.year == bLocal.year &&
      aLocal.month == bLocal.month &&
      aLocal.day == bLocal.day;
}

const double _bottomGap = S.s16;

/// Downward drag distance to dismiss the keyboard.
const double _keyboardDismissDragThreshold = S.s64;

/// How long an incoming message id stays eligible for the entrance animation.
/// Chosen comfortably larger than the animation duration so the tile always
/// has time to mount and play the animation once.
final Duration _animationWindow = Effect.duration(MotionPreset.long);

/// Delay between scroll settling and the floating date header fading out,
/// so the label remains briefly readable after the user stops scrolling.
final Duration _floatingHeaderHideDelay = Effect.duration(MotionPreset.long);

/// Owns a [MessageCubit] for a single message tile.
///
/// Each message gets its own cubit so it can independently manage reactions,
/// editing state, etc. The cubit is keyed by [ValueKey(message.id)] so
/// Flutter reuses the widget when the list rebuilds with the same message.
/// When the message data changes (e.g. content update), the cubit is
/// recreated to pick up the new state.
class _MessageTileCubitHost extends StatefulWidget {
  const _MessageTileCubitHost({
    required this.userCubit,
    required this.message,
    required this.createMessageCubit,
    required this.child,
    super.key,
  });

  final UserCubit userCubit;
  final UiChatMessage message;
  final MessageCubitCreate createMessageCubit;
  final Widget child;

  @override
  State<_MessageTileCubitHost> createState() => _MessageTileCubitHostState();
}

class _MessageTileCubitHostState extends State<_MessageTileCubitHost> {
  late MessageCubit _cubit;

  @override
  void initState() {
    super.initState();
    _cubit = _createCubit();
  }

  @override
  void didUpdateWidget(covariant _MessageTileCubitHost oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Recreate the cubit when the backing data changes so the tile
    // reflects the latest message state (e.g. edited content, new status).
    if (widget.message != oldWidget.message) {
      _cubit.close();
      _cubit = _createCubit();
    }
  }

  @override
  void dispose() {
    _cubit.close();
    super.dispose();
  }

  MessageCubit _createCubit() {
    return widget.createMessageCubit(
      userCubit: widget.userCubit,
      initialState: MessageState(message: widget.message),
    );
  }

  @override
  Widget build(BuildContext context) {
    return BlocProvider<MessageCubit>.value(value: _cubit, child: widget.child);
  }
}
