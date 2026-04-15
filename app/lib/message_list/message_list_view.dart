// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/ui/colors/themes.dart';
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
class _MessageListViewState extends State<MessageListView>
    with WidgetsBindingObserver {
  /// Messages that have already played their entrance animation.
  final _animatedMessages = <MessageId>{};

  final _listController = AnchoredListController();

  /// The last message ID passed to [markAsRead], used to avoid redundant calls
  /// during rapid scroll updates.
  MessageId? _lastMarkedAsReadId;

  MessageListCubit? _commandsCubit;
  StreamSubscription<MessageListCommand>? _commandSubscription;
  bool _initialUnreadScrollHandled = false;

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
  void didChangeDependencies() {
    super.didChangeDependencies();
    final cubit = context.read<MessageListCubit>();
    if (identical(cubit, _commandsCubit)) return;
    _commandSubscription?.cancel();
    _commandsCubit = cubit;
    _commandSubscription = cubit.commands.listen(_handleCommand);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    widget.scrollToBottomController?.onScrollToBottom = null;
    _commandSubscription?.cancel();
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
    if (cubit.state.hasNewer) {
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
        !isAtBottom || cubit.state.hasNewer;
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
    final message = state.messageById(visibleId);
    if (message == null) return;

    _lastMarkedAsReadId = message.id;
    context.read<ChatDetailsCubit>().markAsRead(
      untilMessageId: message.id,
      untilTimestamp: message.timestamp,
    );
  }

  void _handleCommand(MessageListCommand command) {
    switch (command) {
      case MessageListCommand_ScrollToBottom():
        _listController.scrollToBottom(duration: Duration.zero);
      case MessageListCommand_ScrollToId(:final messageId):
        _listController.goToId(messageId);
    }
  }

  void _scheduleInitialUnreadScroll(MessageListStateWrapper state) {
    if (_initialUnreadScrollHandled || state.firstUnreadIndex == null) {
      return;
    }
    final message = state.messageAt(state.firstUnreadIndex!);
    if (message == null) return;

    _initialUnreadScrollHandled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        _listController.goToId(message.id);
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

    return _buildList(composerHeightListenable, state);
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
    Widget buildAnchoredList({double bottomPadding = 0.0}) {
      return AnchoredList<UiChatMessage>(
        data: context.read<MessageListCubit>().messageData,
        controller: _listController,
        idExtractor: (msg) => msg.id,
        bottomPadding: bottomPadding,
        canLoadOlder: state.hasOlder,
        canLoadNewer: state.hasNewer,
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
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                // Disable the auto-scrollbar, we have our own above.
                ScrollConfiguration(
                  behavior: ScrollConfiguration.of(
                    context,
                  ).copyWith(scrollbars: false),
                  child: buildAnchoredList(
                    bottomPadding: composerHeight + _fadeHeight,
                  ),
                ),
                // Gradient fade from transparent to the background color,
                // from 40px above the composer to the screen bottom.
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

  /// Builds a single message row, optionally preceded by the unread divider.
  Widget _buildMessageTile(
    MessageListStateWrapper state,
    UiChatMessage message,
  ) {
    final isNew = state.isNewMessage(message.id);
    // We only want to animate a message if it's new and hasn't already played
    // its animation.
    final shouldAnimate = isNew && _animatedMessages.add(message.id);

    final isFirstUnread =
        state.firstUnreadIndex != null &&
        state.messageAt(state.firstUnreadIndex!)?.id == message.id;

    Widget tile = _MessageTileCubitHost(
      key: ValueKey(message.id),
      userCubit: context.read<UserCubit>(),
      message: message,
      createMessageCubit: widget.createMessageCubit,
      child: ChatTile(
        isConnectionChat: state.isConnectionChat ?? false,
        animated: isNew,
        shouldAnimate: shouldAnimate,
      ),
    );

    // Insert "N unread messages" divider above the first unread message.
    if (isFirstUnread) {
      final unreadCount = state.messageData.length - state.firstUnreadIndex!;
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
