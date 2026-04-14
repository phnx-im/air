// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/anchored_list/anchored_list.dart';
import 'package:air/widgets/anchored_list/controller.dart';

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

/// Integrates [AnchoredList] with [MessageListCubit] to display a paginated,
/// scroll-stable chat message list.
///
/// Responsibilities beyond rendering:
///  - Drives the scroll-to-bottom FAB via [ScrollToBottomController].
///  - Marks the conversation as read up to the newest visible message.
///  - Routes cubit scroll-to-index commands to the [AnchoredListController].
///  - Tracks entrance animations so each message animates in only once.
class _MessageListViewState extends State<MessageListView>
    with WidgetsBindingObserver {
  /// Messages that have already played their entrance animation.
  final _animatedMessages = <MessageId>{};

  final _listController = AnchoredListController();

  /// The last message ID passed to [markAsRead], used to avoid redundant calls
  /// during rapid scroll updates.
  MessageId? _lastMarkedAsReadId;

  /// The state object that last triggered a scroll-to-index action.
  /// Compared by identity so a new emission with the same scrollToIndex
  /// still triggers a scroll.
  MessageListState? _lastScrolledState;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    widget.scrollToBottomController?.onScrollToBottom = _scrollToBottom;

    // Drive the scroll-to-bottom button from isAtBottom + hasNewer.
    _listController.isAtBottom.addListener(_updateShowButton);
    _listController.newestVisibleId.addListener(
      _markCurrentVisibleMessageAsRead,
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    widget.scrollToBottomController?.onScrollToBottom = null;
    _listController.isAtBottom.removeListener(_updateShowButton);
    _listController.newestVisibleId.removeListener(
      _markCurrentVisibleMessageAsRead,
    );
    _listController.dispose();
    super.dispose();
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
    if (cubit.state.meta.hasNewer) {
      cubit.jumpToBottom();
    } else {
      _listController.scrollToBottom();
    }
  }

  /// Shows the scroll-to-bottom button when the user has scrolled away
  /// from the bottom or when there are newer messages not yet loaded.
  void _updateShowButton() {
    final cubit = context.read<MessageListCubit>();
    final isAtBottom = _listController.isAtBottom.value;
    widget.scrollToBottomController?.showButton.value =
        !isAtBottom || cubit.state.meta.hasNewer;
  }

  /// Marks the conversation as read up to the newest message currently visible
  /// in the viewport.
  ///
  /// The anchored list computes actual visible items from its measured layout
  /// and exposes the newest visible ID via its controller, so this avoids the
  /// old fixed-height approximation based on scroll offset alone.
  void _markCurrentVisibleMessageAsRead() {
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
    final index = state.messageIdIndex(visibleId);
    if (index == null) return;
    final message = state.messageAt(index);
    if (message == null) return;

    _lastMarkedAsReadId = message.id;
    context.read<ChatDetailsCubit>().markAsRead(
      untilMessageId: message.id,
      untilTimestamp: message.timestamp,
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = context.select((MessageListCubit cubit) => cubit.state);

    // Clean up stale animation tracking for messages no longer loaded.
    _animatedMessages.removeWhere((id) => state.messageIdIndex(id) == null);

    // Deferred to avoid side-effects during build.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _updateShowButton();
    });

    final composerHeightListenable =
        widget.scrollToBottomController?.composerHeight;

    // Translate cubit scroll-to-index commands into AnchoredList actions.
    // The cubit sets scrollToIndex when it wants the UI to navigate
    // (e.g. after jumpToMessage or initial load with an unread divider).
    return BlocListener<MessageListCubit, MessageListState>(
      listenWhen: (prev, curr) =>
          curr.meta.scrollToIndex != null &&
          !identical(curr, _lastScrolledState),
      listener: (context, state) {
        _lastScrolledState = state;
        final scrollTo = state.meta.scrollToIndex!;
        context.read<MessageListCubit>().clearScrollToIndex();
        final message = state.messageAt(scrollTo);
        if (message == null) return;

        // If we're already at the bottom and the target is the newest
        // message, jump instantly (no animation) to avoid a visible
        // flicker. Otherwise, navigate by ID — AnchoredList handles
        // both visible-item animation and off-screen iterative jumping.
        final isNewest = scrollTo == state.loadedMessagesCount - 1;
        if (state.meta.isAtBottom && isNewest) {
          _listController.scrollToBottom(duration: Duration.zero);
        } else {
          _listController.goToId(message.id);
        }
      },
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
    MessageListState state,
  ) {
    Widget buildAnchoredList({double bottomPadding = 0.0}) {
      return AnchoredList<UiChatMessage>(
        data: context.read<MessageListCubit>().messageData,
        controller: _listController,
        idExtractor: (msg) => msg.id,
        bottomPadding: bottomPadding,
        canLoadOlder: state.meta.hasOlder,
        canLoadNewer: state.meta.hasNewer,
        onLoadOlder: () {
          context.read<MessageListCubit>().loadOlder();
        },
        onLoadNewer: () {
          context.read<MessageListCubit>().loadNewer();
        },
        onLoadAround: (id) async {
          if (id is MessageId) {
            await context.read<MessageListCubit>().jumpToMessage(messageId: id);
          }
        },
        itemBuilder: (context, message, index) {
          return _buildMessageTile(state, message);
        },
      );
    }

    if (composerHeightListenable == null) {
      return buildAnchoredList();
    }
    return ValueListenableBuilder<double>(
      valueListenable: composerHeightListenable,
      builder: (context, height, _) => buildAnchoredList(bottomPadding: height),
    );
  }

  /// Builds a single message row, optionally preceded by the unread divider.
  Widget _buildMessageTile(MessageListState state, UiChatMessage message) {
    // Only animate a message's entrance once — mark it as seen.
    final animate =
        !_animatedMessages.contains(message.id) &&
        state.isNewMessage(message.id);
    if (animate) {
      _animatedMessages.add(message.id);
    }

    final index = state.messageIdIndex(message.id);
    final isFirstUnread =
        state.meta.firstUnreadIndex != null &&
        index == state.meta.firstUnreadIndex;

    Widget tile = _MessageTileCubitHost(
      key: ValueKey(message.id),
      userCubit: context.read<UserCubit>(),
      message: message,
      createMessageCubit: widget.createMessageCubit,
      child: ChatTile(
        isConnectionChat: state.meta.isConnectionChat ?? false,
        animated: animate,
      ),
    );

    // Insert "N unread messages" divider above the first unread message.
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

    return tile;
  }
}

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
