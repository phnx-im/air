// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:flutter/rendering.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import 'controller.dart';
import 'data.dart';
import 'height_cache.dart';
import 'jump_state.dart';

/// A scrollable list anchored to the bottom, designed for chat UIs.
///
/// Uses a reversed [CustomScrollView] so index 0 sits at the bottom and
/// scroll offset 0 means "fully scrolled to newest". This natural mapping
/// lets standard scroll physics handle stick-to-bottom without custom
/// simulation.
///
/// ## Scroll-position stability
///
/// When items are inserted or removed *outside* the viewport (e.g. older
/// messages loaded via pagination), the widget applies a layout correction
/// so the user's reading position doesn't shift. The correction is computed
/// inside [ScrollPosition.correctForNewDimensions] — the only Flutter hook
/// that fires between layout and paint — by comparing an anchor item's
/// position before and after the change.
///
/// ## Pagination
///
/// Fires [onLoadOlder] / [onLoadNewer] when the scroll position enters a
/// buffer zone near either edge. The buffer is at least
/// [paginationThreshold] or two viewports, whichever is larger, so that
/// page loads typically complete before the user reaches blank space.
///
/// ## Jump-to-message
///
/// Uses a three-phase state machine (idle → loading → scrolling → idle).
/// If the target item isn't loaded yet, the widget shows [loadingBuilder]
/// while [onLoadAround] fetches it, then scrolls once the data arrives.
class AnchoredList<T> extends StatefulWidget {
  const AnchoredList({
    required this.data,
    required this.itemBuilder,
    required this.idExtractor,
    this.controller,
    this.canLoadOlder = true,
    this.canLoadNewer = true,
    this.onLoadOlder,
    this.onLoadNewer,
    this.onLoadAround,
    this.paginationThreshold = 500.0,
    this.stickToBottomThreshold = 50.0,
    this.loadingBuilder,
    this.physics,
    this.topPadding = 0.0,
    this.bottomPadding = 0.0,
    super.key,
  });

  /// The data model driving this list. Mutations on it emit diffs that the
  /// widget consumes each frame to compute layout corrections.
  final AnchoredListData<T> data;
  final Widget Function(BuildContext context, T item, int index) itemBuilder;

  /// Extracts a stable, unique identity from an item. Used as the key for
  /// height caching, child-index lookup, and anchor resolution.
  final Object Function(T) idExtractor;
  final AnchoredListController? controller;
  final bool canLoadOlder;
  final bool canLoadNewer;
  final VoidCallback? onLoadOlder;
  final VoidCallback? onLoadNewer;

  /// Called when a jump-to-message targets an ID not present in [data].
  /// The caller should load messages around this ID and mutate [data].
  final Future<void> Function(Object id)? onLoadAround;

  /// Minimum distance from an edge (in pixels) before a pagination
  /// callback fires. The actual buffer is max(this, 2× viewport height).
  final double paginationThreshold;

  /// If the user is within this many pixels of the bottom when a new
  /// message arrives at index 0, the list auto-scrolls to show it.
  final double stickToBottomThreshold;
  final WidgetBuilder? loadingBuilder;
  final ScrollPhysics? physics;
  final double topPadding;
  final double bottomPadding;

  @override
  State<AnchoredList<T>> createState() => _AnchoredListState<T>();
}

class _AnchoredListState<T> extends State<AnchoredList<T>> {
  late _AnchoredScrollController _scrollController;
  late AnchoredListController _controller;
  bool _ownsController = false;
  final AnchoredListHeightCache _heightCache = AnchoredListHeightCache();
  final AnchoredListJumpState _jumpState = AnchoredListJumpState();
  final GlobalKey _viewportKey = GlobalKey();
  final GlobalKey _sliverKey = GlobalKey();

  /// Reverse lookup from item identity to its current index in [data].
  /// Rebuilt on every data change so anchor resolution and child-index
  /// callbacks are O(1).
  Map<Object, int> _idToIndex = {};

  /// Set between [_onDataChanged] (which captures the pre-layout snapshot)
  /// and [_resolvePendingLayoutCorrection] (which runs during layout). Null
  /// when no correction is needed.
  _LayoutCorrectionRequest<T>? _pendingLayoutCorrection;

  /// De-duplication guards: prevent firing a pagination callback while the
  /// previous page load is still in flight. Reset when new data arrives.
  bool _loadingOlder = false;
  bool _loadingNewer = false;

  bool _pendingAnimateToBottom = false;

  // Frame-callback guards — ensure at most one callback per type is
  // scheduled at a time, avoiding redundant post-frame work.
  bool _pendingCommandFrameCallbackScheduled = false;
  bool _pendingJumpExecutionFrameCallbackScheduled = false;
  bool _pendingViewportStateFrameCallbackScheduled = false;

  static const _animateDuration = Duration(milliseconds: 300);
  static const _animateCurve = Curves.easeOut;

  /// Sub-pixel threshold: differences smaller than this are treated as
  /// equal to avoid infinite correction loops from floating-point drift.
  static const _jumpAlignmentTolerance = 0.5;

  /// Maximum iterative jump attempts when scrolling to an off-screen item.
  /// Each attempt refines the estimate as newly-laid-out items update the
  /// height cache. 8 is generous — most jumps converge in 2–3 iterations.
  static const _maxOffscreenJumpAttempts = 8;

  @override
  void initState() {
    super.initState();
    _scrollController = _AnchoredScrollController(
      resolveLayoutCorrection: _resolvePendingLayoutCorrection,
    );
    _attachController(widget.controller);
    _rebuildIdToIndex();
    widget.data.addListener(_onDataChanged);
    _scheduleViewportStateRefresh();
  }

  @override
  void didUpdateWidget(covariant AnchoredList<T> oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      _pendingLayoutCorrection = null;
      oldWidget.data.removeListener(_onDataChanged);
      widget.data.addListener(_onDataChanged);
      _rebuildIdToIndex();
    }
    if (oldWidget.controller != widget.controller) {
      _detachController();
      _attachController(widget.controller);
    }
    if (!widget.canLoadOlder) {
      _loadingOlder = false;
    }
    if (!widget.canLoadNewer) {
      _loadingNewer = false;
    }
    if ((oldWidget.canLoadOlder != widget.canLoadOlder ||
            oldWidget.canLoadNewer != widget.canLoadNewer) &&
        _scrollController.hasClients) {
      _schedulePaginationCheck();
    }
    _scheduleViewportStateRefresh();
  }

  @override
  void dispose() {
    widget.data.removeListener(_onDataChanged);
    _detachController();
    _scrollController.dispose();
    super.dispose();
  }

  void _attachController(AnchoredListController? external) {
    if (external != null) {
      _controller = external;
      _ownsController = false;
    } else {
      _controller = AnchoredListController();
      _ownsController = true;
    }
    _controller.scrollController = _scrollController;
    _controller.addListener(_onControllerCommand);
    _schedulePendingCommandProcessing();
  }

  void _detachController() {
    _controller.removeListener(_onControllerCommand);
    _controller.scrollController = null;
    _pendingCommandFrameCallbackScheduled = false;
    if (_ownsController) {
      _controller.dispose();
    }
  }

  void _rebuildIdToIndex() {
    final newMap = <Object, int>{};
    final items = widget.data.items;
    for (var i = 0; i < items.length; i++) {
      newMap[widget.idExtractor(items[i])] = i;
    }
    _idToIndex = newMap;
  }

  // -- Scroll correction engine --
  //
  // When items are inserted/removed outside the viewport, Flutter's default
  // behaviour shifts the visible content because the scroll extent changes
  // but the pixel offset doesn't. The correction engine fixes this:
  //
  //   1. _onDataChanged captures a snapshot of which items are visible and
  //      their viewport-relative positions *before* the framework re-lays-out.
  //   2. During layout, Flutter calls correctForNewDimensions on our custom
  //      ScrollPosition. We intercept this to run _resolvePendingLayoutCorrection.
  //   3. The resolver picks an anchor item from the snapshot, measures where
  //      it ended up after layout, and shifts the scroll offset by the delta
  //      so the anchor stays in the same visual position.

  void _onDataChanged() {
    final diffs = widget.data.drainDiffs();
    if (diffs.isEmpty) return;

    // Data arrived — allow the next pagination request.
    _loadingOlder = false;
    _loadingNewer = false;

    final isReload = diffs.any((d) => d is AnchoredListReload<T>);
    if (isReload) {
      _pendingLayoutCorrection = null;
      _heightCache.clear();
    } else {
      // Stick-to-bottom: if the user is near the bottom and a new message
      // is inserted at index 0 (the newest position), animate down to show
      // it. This check must happen BEFORE the layout correction snapshot,
      // because the correction would fight the animate-to-bottom.
      final hasInsertAtZero = diffs.any(
        (d) => d is AnchoredListInsert<T> && d.index == 0,
      );
      if (hasInsertAtZero && _scrollController.hasClients) {
        final pixels = _scrollController.position.pixels;
        if (pixels > 0 && pixels <= widget.stickToBottomThreshold) {
          _pendingAnimateToBottom = true;
        }
      }

      // Only attempt layout correction when:
      // - we're not about to animate to bottom (would conflict),
      // - no jump is in progress (jump manages its own positioning),
      // - we're not already at offset 0 (nothing to shift).
      final canApplyLayoutCorrection =
          !_pendingAnimateToBottom &&
          _jumpState.phase == AnchoredListJumpPhase.idle &&
          _scrollController.hasClients &&
          _scrollController.position.pixels > 0.0;
      _pendingLayoutCorrection = canApplyLayoutCorrection
          ? _captureLayoutCorrectionRequest(diffs)
          : null;

      _syncAnchoredListHeightCache(diffs);
    }

    _rebuildIdToIndex();

    // If we were waiting for data (jump phase == loading) and the target
    // item just appeared, advance the state machine to scrolling.
    final transitioned =
        _jumpState.phase == AnchoredListJumpPhase.loading &&
        _jumpState.onDataUpdated((id) => _idToIndex.containsKey(id));

    setState(() {});
    _scheduleViewportStateRefresh();

    if (transitioned) {
      _scheduleJumpExecution();
    }

    if (isReload) {
      return;
    }

    if (_pendingAnimateToBottom) {
      _pendingAnimateToBottom = false;
      SchedulerBinding.instance.addPostFrameCallback((_) {
        if (_scrollController.hasClients) {
          _scrollController.animateTo(
            0.0,
            duration: const Duration(milliseconds: 150),
            curve: Curves.easeOut,
          );
        }
      });
    }

    // After data changes the position relative to edges may have changed.
    // Re-check pagination so the next page is loaded proactively, avoiding
    // a visible gap when scrolling toward the insert point (newer end).
    _schedulePaginationCheck();
  }

  void _schedulePaginationCheck() {
    SchedulerBinding.instance.addPostFrameCallback((_) {
      if (mounted && _scrollController.hasClients) {
        _checkPagination(_scrollController.position);
      }
    });
  }

  /// Captures the current viewport state alongside the diffs so
  /// [_resolvePendingLayoutCorrection] can compare before/after layout.
  _LayoutCorrectionRequest<T>? _captureLayoutCorrectionRequest(
    List<AnchoredListDiff<T>> diffs,
  ) {
    final snapshot = _captureViewportSnapshot();
    if (snapshot == null) return null;
    return _LayoutCorrectionRequest(
      snapshot: snapshot,
      diffs: List<AnchoredListDiff<T>>.of(diffs),
    );
  }

  _ViewportSnapshot? _captureViewportSnapshot() {
    if (!_scrollController.hasClients) return null;

    final visibleItems = _visibleItemSnapshots();
    if (visibleItems.isEmpty) return null;
    return _ViewportSnapshot(visibleItems);
  }

  /// Core of the scroll-position stability engine.
  ///
  /// Called from [_AnchoredScrollPosition.correctForNewDimensions] during
  /// layout — the only point where we can adjust pixels without triggering
  /// a second layout pass.
  ///
  /// Returns the corrected pixel offset, or null to let Flutter apply its
  /// default clamping.
  double? _resolvePendingLayoutCorrection(
    ScrollMetrics _,
    ScrollMetrics newPosition,
  ) {
    final request = _pendingLayoutCorrection;
    if (request == null || !mounted) return null;

    // If the diffs touch items *inside* the viewport, a content-aware
    // correction isn't meaningful — the viewport itself changed.
    if (_diffsAffectViewport(request.snapshot, request.diffs)) {
      _pendingLayoutCorrection = null;
      return null;
    }

    // Find an anchor: a visible item from the pre-change snapshot that
    // is still rendered after the rebuild. Prefer the bottom-most item
    // because it's the user's active reading position.
    final anchor = _findAnchorAfterRebuild(request.snapshot);
    if (anchor == null) {
      _pendingLayoutCorrection = null;
      return null;
    }

    // Measure how far the anchor drifted from its pre-change position.
    final currentTop = _itemTopInViewport(anchor.id);
    if (currentTop == null) {
      _pendingLayoutCorrection = null;
      return null;
    }

    final delta = currentTop - anchor.top;
    if (delta.abs() < 0.5) {
      _pendingLayoutCorrection = null;
      return null;
    }

    // Shift the scroll offset by the inverse of the drift so the anchor
    // returns to its original visual position.
    final targetPixels = (newPosition.pixels - delta).clamp(
      newPosition.minScrollExtent,
      newPosition.maxScrollExtent,
    );

    if ((targetPixels - newPosition.pixels).abs() < 0.5) {
      _pendingLayoutCorrection = null;
      return null;
    }

    _pendingLayoutCorrection = null;
    return targetPixels.toDouble();
  }

  /// Determines whether any diff touches items currently visible in the
  /// viewport. If so, the layout correction is skipped because the user
  /// can *see* the change — correcting would fight the visual update.
  ///
  /// The visible index range is tracked as a sliding window: inserts and
  /// removes before the window shift it forward/backward so subsequent
  /// diffs are tested against the correct indices.
  bool _diffsAffectViewport(
    _ViewportSnapshot snapshot,
    List<AnchoredListDiff<T>> diffs,
  ) {
    var visibleStart = snapshot.lowestVisibleIndex;
    var visibleEnd = snapshot.highestVisibleIndex;

    for (final diff in diffs) {
      switch (diff) {
        case AnchoredListInsert<T>(:final index, :final count):
          // Insert before viewport: shift the visible window up.
          if (index <= visibleStart) {
            visibleStart += count;
            visibleEnd += count;
            continue;
          }
          // Insert after viewport: no effect.
          if (index > visibleEnd) {
            continue;
          }
          // Insert inside viewport: affects visible content.
          return true;
        case AnchoredListRemove<T>(:final index, :final count):
          final removedEnd = index + count - 1;
          // Remove entirely before viewport: shift the window down.
          if (removedEnd < visibleStart) {
            visibleStart -= count;
            visibleEnd -= count;
            continue;
          }
          // Remove entirely after viewport: no effect.
          if (index > visibleEnd) {
            continue;
          }
          // Overlaps viewport.
          return true;
        case AnchoredListUpdate<T>(:final index):
          if (index >= visibleStart && index <= visibleEnd) {
            return true;
          }
        case AnchoredListReload<T>():
          return true;
      }
    }

    return false;
  }

  /// Iterates anchor candidates (bottom-most first) and returns the first
  /// one that's still rendered after the rebuild. Bottom-most is preferred
  /// because it's closest to the user's focus in a chat.
  _VisibleItemSnapshot? _findAnchorAfterRebuild(_ViewportSnapshot snapshot) {
    for (final item in snapshot.anchorCandidates) {
      if (_itemTopInViewport(item.id) != null) {
        return item;
      }
    }
    return null;
  }

  /// Returns the top-edge position of the item with [id] in viewport
  /// coordinates, or null if the item isn't currently laid out or is
  /// fully outside the visible area.
  double? _itemTopInViewport(Object id) {
    final itemBox = _renderBoxForId(id);
    final viewportBox = itemBox != null ? _viewportForItemBox(itemBox) : null;
    if (viewportBox == null || itemBox == null) return null;

    final top = _itemTopInViewportBox(itemBox, id);
    if (top == null) return null;
    final bottom = top + _estimatedHeight(id);
    // Fully above or fully below the viewport.
    if (bottom <= 0.0 || top >= viewportBox.size.height) {
      return null;
    }
    return top;
  }

  /// Computes an item's top-edge position in viewport coordinates by
  /// combining the sliver's paint offset with the child's main-axis
  /// position within the sliver.
  ///
  /// Handles reversed growth direction (our scroll view is reversed)
  /// by flipping the child position relative to the sliver's paint extent.
  double? _itemTopInViewportBox(RenderBox itemBox, Object id) {
    final sliver = itemBox.parent;
    if (sliver is! RenderSliverMultiBoxAdaptor || sliver.geometry == null) {
      return null;
    }

    final sliverParentData = sliver.parentData;
    if (sliverParentData is! SliverPhysicalParentData) {
      return null;
    }

    // childMainAxisPosition gives the offset from the sliver's zero edge.
    // In a reversed list the zero edge is at the bottom, so we flip.
    var delta = sliver.childMainAxisPosition(itemBox);
    final reversed = axisDirectionIsReversed(sliver.constraints.axisDirection);
    final rightWayUp = switch (sliver.constraints.growthDirection) {
      GrowthDirection.forward => !reversed,
      GrowthDirection.reverse => reversed,
    };

    if (!rightWayUp) {
      // Flip: convert from bottom-relative to top-relative.
      delta = sliver.geometry!.paintExtent - _estimatedHeight(id) - delta;
    }

    return switch (sliver.constraints.axis) {
      Axis.vertical => sliverParentData.paintOffset.dy + delta,
      Axis.horizontal => sliverParentData.paintOffset.dx + delta,
    };
  }

  /// Keeps the exact-height cache consistent with data mutations.
  /// Inserts don't need action (heights are recorded on layout).
  /// Removes and identity-changing updates evict stale entries.
  void _syncAnchoredListHeightCache(List<AnchoredListDiff<T>> diffs) {
    for (final diff in diffs) {
      switch (diff) {
        case AnchoredListInsert<T>():
          break;
        case AnchoredListRemove<T>(:final removedItems):
          for (final item in removedItems) {
            _heightCache.remove(widget.idExtractor(item));
          }
        case AnchoredListUpdate<T>(:final oldItem, :final index):
          // Only evict when the identity changed — same-id updates will
          // get a fresh measurement on the next layout pass.
          final oldId = widget.idExtractor(oldItem);
          if (index >= 0 && index < widget.data.length) {
            final newId = widget.idExtractor(widget.data[index]);
            if (newId != oldId) {
              _heightCache.remove(oldId);
            }
          }
        case AnchoredListReload<T>():
          _heightCache.clear();
      }
    }
  }

  RenderBox? get _viewportBox =>
      _renderBoxForContext(_viewportKey.currentContext);

  RenderSliverMultiBoxAdaptor? get _sliver {
    final renderObject = _sliverKey.currentContext?.findRenderObject();
    return renderObject is RenderSliverMultiBoxAdaptor ? renderObject : null;
  }

  RenderBox? _viewportForItemBox(RenderBox itemBox) {
    final viewportObject = RenderAbstractViewport.maybeOf(itemBox);
    if (viewportObject case final RenderBox viewport
        when viewport.attached && viewport.hasSize) {
      return viewport;
    }
    return _viewportBox;
  }

  RenderBox? _renderBoxForContext(BuildContext? context) {
    if (context is Element && !context.mounted) {
      return null;
    }
    RenderObject? renderObject;
    try {
      renderObject = context?.findRenderObject();
    } on FlutterError {
      return null;
    }
    if (renderObject is RenderBox &&
        renderObject.attached &&
        renderObject.hasSize) {
      return renderObject;
    }
    return null;
  }

  /// Walks the sliver's child list to find the RenderBox at [id]'s index.
  /// This is a linear scan over currently-laid-out children (typically
  /// viewport-sized, so ~10–30 items). We can't use the sliver's internal
  /// index lookup because [RenderSliverMultiBoxAdaptor] doesn't expose it.
  RenderBox? _renderBoxForId(Object id) {
    final targetIndex = _idToIndex[id];
    if (targetIndex == null) return null;

    RenderBox? child = _sliver?.firstChild;
    while (child != null) {
      final parentData = child.parentData;
      if (parentData is SliverMultiBoxAdaptorParentData &&
          parentData.index == targetIndex) {
        return child;
      }
      child = _sliver?.childAfter(child);
    }
    return null;
  }

  void _onControllerCommand() {
    if (!mounted) return;
    if (_scrollController.hasClients) {
      _processPendingCommand();
      return;
    }
    _schedulePendingCommandProcessing();
  }

  void _schedulePendingCommandProcessing() {
    if (_pendingCommandFrameCallbackScheduled || !mounted) {
      return;
    }

    _pendingCommandFrameCallbackScheduled = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _pendingCommandFrameCallbackScheduled = false;
      if (!mounted || !_scrollController.hasClients) {
        return;
      }
      _processPendingCommand();
    });
  }

  void _scheduleJumpExecution() {
    if (_pendingJumpExecutionFrameCallbackScheduled || !mounted) {
      return;
    }

    _pendingJumpExecutionFrameCallbackScheduled = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _pendingJumpExecutionFrameCallbackScheduled = false;
      if (!mounted || _jumpState.phase != AnchoredListJumpPhase.scrolling) {
        return;
      }
      if (!_scrollController.hasClients) {
        _scheduleJumpExecution();
        return;
      }
      _executeJumpScroll();
    });
  }

  void _scheduleViewportStateRefresh() {
    if (_pendingViewportStateFrameCallbackScheduled || !mounted) {
      return;
    }

    _pendingViewportStateFrameCallbackScheduled = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _pendingViewportStateFrameCallbackScheduled = false;
      if (!mounted) return;
      _refreshViewportState();
    });
  }

  void _refreshViewportState() {
    if (!_scrollController.hasClients) {
      _controller.isAtBottomNotifier.value = true;
      _controller.newestVisibleIdNotifier.value = null;
      return;
    }
    _updateIsAtBottom(_scrollController.position);
    _updateNewestVisibleId();
  }

  // -- Pagination --

  bool _handleScrollNotification(ScrollNotification notification) {
    if (notification is ScrollUpdateNotification ||
        notification is ScrollEndNotification) {
      _checkPagination(notification.metrics);
      _updateIsAtBottom(notification.metrics);
      _updateNewestVisibleId();
      _scheduleViewportStateRefresh();
    }
    _processPendingCommand();
    return false;
  }

  /// Fires pagination callbacks when the scroll position is within the
  /// buffer zone of either edge.
  ///
  /// Because the scroll view is *reversed*, the index/pixel mapping is
  /// inverted relative to visual direction:
  ///   - pixels ≈ maxScrollExtent → visual top → older messages
  ///   - pixels ≈ 0               → visual bottom → newer messages
  void _checkPagination(ScrollMetrics metrics) {
    final paginationBuffer = _paginationBuffer(metrics);
    final nearTop =
        widget.canLoadOlder &&
        metrics.pixels >= metrics.maxScrollExtent - paginationBuffer;
    if (nearTop && !_loadingOlder && widget.onLoadOlder != null) {
      _loadingOlder = true;
      widget.onLoadOlder!();
    }
    final nearBottom =
        widget.canLoadNewer && metrics.pixels <= paginationBuffer;
    if (nearBottom && !_loadingNewer && widget.onLoadNewer != null) {
      _loadingNewer = true;
      widget.onLoadNewer!();
    }
  }

  double _paginationBuffer(ScrollMetrics metrics) {
    // Keep a multi-screen buffer before either edge so pagination has time to
    // land before the user reaches the unloaded boundary in the windowed list.
    return math.max(widget.paginationThreshold, metrics.viewportDimension * 2);
  }

  void _updateIsAtBottom(ScrollMetrics metrics) {
    _controller.isAtBottomNotifier.value =
        metrics.pixels <= widget.stickToBottomThreshold;
  }

  void _updateNewestVisibleId() {
    final visibleItems = _visibleItemSnapshots();
    final newestVisible = visibleItems.isEmpty ? null : visibleItems.last.id;
    if (_controller.newestVisibleIdNotifier.value != newestVisible) {
      _controller.newestVisibleIdNotifier.value = newestVisible;
    }
  }

  // -- Jump-to-message --
  //
  // Two-step process:
  //   1. If the target is already in data, jump directly (scrolling phase).
  //   2. If not, ask the caller to load it (loading phase), then jump once
  //      _onDataChanged detects the target ID has appeared.
  //
  // The scroll itself may require multiple layout passes because item
  // heights are only known after layout. _jumpToOffscreenTarget handles
  // this with an iterative refinement loop.

  void _processPendingCommand() {
    final cmd = _controller.drainCommand();
    if (cmd == null) return;

    switch (cmd) {
      case GoToIdCommand(:final id):
        _jumpState.requestJump(
          id,
          isIdLoaded: (id) => _idToIndex.containsKey(id),
          onLoadAround: (id) => widget.onLoadAround?.call(id),
        );
        if (_jumpState.phase == AnchoredListJumpPhase.scrolling) {
          _executeJumpScroll();
        } else {
          setState(() {});
        }
      case ScrollToBottomCommand(:final duration, :final curve):
        if (duration == Duration.zero) {
          _scrollController.jumpTo(0.0);
        } else {
          _scrollController.animateTo(
            0.0,
            duration: duration ?? _animateDuration,
            curve: curve ?? _animateCurve,
          );
        }
    }
  }

  /// Estimates the scroll offset that would place item at [index] at the
  /// top of the viewport. Because the list is reversed (offset 0 = bottom),
  /// we need: item's cumulative offset − viewport height + item height.
  double _topAlignedOffset(int index) {
    final offset = _estimateOffsetAtIndex(index);
    final targetId = widget.idExtractor(widget.data[index]);
    final itemHeight = _estimatedHeight(targetId);
    final viewportBox = _viewportBox;
    if (viewportBox == null) return offset;
    final viewportHeight = viewportBox.size.height;
    return offset - viewportHeight + itemHeight;
  }

  void _executeJumpScroll() {
    final targetId = _jumpState.targetId;
    if (targetId == null) return;

    final index = _idToIndex[targetId];
    if (index == null) return;

    final currentTop = _itemTopInViewport(targetId);
    if (currentTop != null) {
      _alignVisibleTarget(targetId, animate: true);
      return;
    }

    _jumpToOffscreenTarget(targetId, index, attempt: 0);
  }

  /// Scrolls so that [targetId] sits at the top of the viewport.
  /// Used when the item is already laid out and visible — the offset
  /// needed is simply current pixels minus the item's current top.
  void _alignVisibleTarget(Object targetId, {required bool animate}) {
    if (!_scrollController.hasClients || _jumpState.targetId != targetId) {
      return;
    }

    final currentTop = _itemTopInViewport(targetId);
    if (currentTop == null) return;

    // Shift the scroll position by exactly how far the item is from
    // the viewport's top edge.
    final desiredOffset = _scrollController.position.pixels - currentTop;
    final clampedOffset = desiredOffset
        .clamp(
          _scrollController.position.minScrollExtent,
          _scrollController.position.maxScrollExtent,
        )
        .toDouble();

    if ((clampedOffset - _scrollController.position.pixels).abs() <
        _jumpAlignmentTolerance) {
      _jumpState.onScrollComplete();
      return;
    }

    if (!animate) {
      _scrollController.jumpTo(clampedOffset);
      _jumpState.onScrollComplete();
      return;
    }

    _scrollController
        .animateTo(
          clampedOffset,
          duration: _animateDuration,
          curve: _animateCurve,
        )
        .then((_) => _jumpState.onScrollComplete());
  }

  /// Iteratively jumps toward an off-screen item.
  ///
  /// The first jump uses an estimated offset (from the height cache).
  /// After each jump, Flutter lays out the newly-visible items, updating
  /// the height cache. On the next iteration the estimate is more accurate.
  /// This converges in 2–3 passes for typical chat heights; the attempt
  /// counter is a safety net.
  ///
  /// Once the target is laid out and visible, we hand off to
  /// [_alignVisibleTarget] for pixel-perfect placement.
  void _jumpToOffscreenTarget(
    Object targetId,
    int index, {
    required int attempt,
  }) {
    if (!_scrollController.hasClients || _jumpState.targetId != targetId) {
      return;
    }

    final clampedOffset = _topAlignedOffset(index)
        .clamp(
          _scrollController.position.minScrollExtent,
          _scrollController.position.maxScrollExtent,
        )
        .toDouble();
    final previousPixels = _scrollController.position.pixels;

    if ((clampedOffset - previousPixels).abs() >= _jumpAlignmentTolerance) {
      _scrollController.jumpTo(clampedOffset);
    }

    if (attempt >= _maxOffscreenJumpAttempts) {
      _jumpState.onScrollComplete();
      return;
    }

    // Wait for the framework to lay out at the new position, then check
    // whether the target is now visible or we need another iteration.
    SchedulerBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scrollController.hasClients) return;
      if (_jumpState.targetId != targetId) return;

      if (_itemTopInViewport(targetId) != null) {
        _alignVisibleTarget(targetId, animate: false);
        return;
      }

      // Detect convergence: if neither the estimated offset nor the actual
      // position changed, further iterations won't help.
      final nextOffset = _topAlignedOffset(index)
          .clamp(
            _scrollController.position.minScrollExtent,
            _scrollController.position.maxScrollExtent,
          )
          .toDouble();
      final isStuck =
          (nextOffset - _scrollController.position.pixels).abs() <
              _jumpAlignmentTolerance &&
          (nextOffset - clampedOffset).abs() < _jumpAlignmentTolerance;
      if (isStuck) {
        _jumpState.onScrollComplete();
        return;
      }

      _jumpToOffscreenTarget(targetId, index, attempt: attempt + 1);
    });
    SchedulerBinding.instance.ensureVisualUpdate();
  }

  // -- Build --

  @override
  Widget build(BuildContext context) {
    _scheduleViewportStateRefresh();
    if (_jumpState.phase == AnchoredListJumpPhase.loading &&
        widget.loadingBuilder != null) {
      return widget.loadingBuilder!(context);
    }

    // SizedBox.expand gives us a measured viewport box for coordinate math.
    // CustomScrollView is reversed so index 0 is visually at the bottom.
    return NotificationListener<ScrollNotification>(
      onNotification: _handleScrollNotification,
      child: SizedBox.expand(
        key: _viewportKey,
        child: CustomScrollView(
          reverse: true,
          controller: _scrollController,
          physics: widget.physics ?? const ClampingScrollPhysics(),
          slivers: [_buildSliverList()],
        ),
      ),
    );
  }

  Widget _buildSliverList() {
    final sliverList = SliverList(
      key: _sliverKey,
      delegate: _HeightCachingDelegate(
        builder: _buildItem,
        childCount: widget.data.length,
        findChildIndexCallback: _findChildIndex,
        heightCache: _heightCache,
      ),
    );
    final topPadding = widget.topPadding;
    final bottomPadding = widget.bottomPadding;
    if (topPadding == 0.0 && bottomPadding == 0.0) return sliverList;
    return SliverPadding(
      padding: EdgeInsets.only(top: topPadding, bottom: bottomPadding),
      sliver: sliverList,
    );
  }

  Widget _buildItem(BuildContext context, int index) {
    final item = widget.data[index];
    final id = widget.idExtractor(item);
    return _MeasuredItem(
      key: ValueKey<Object>(id),
      id: id,
      cache: _heightCache,
      child: widget.itemBuilder(context, item, index),
    );
  }

  /// Called by [SliverChildBuilderDelegate.findChildIndexCallback] to
  /// resolve a child's key back to its current index. This lets the
  /// framework reuse existing Elements when items move positions (e.g.
  /// after an insert shifts indices).
  int? _findChildIndex(Key key) {
    if (key is ValueKey) {
      return _idToIndex[key.value];
    }
    return null;
  }

  /// Walks all currently-laid-out children and returns those whose paint
  /// bounds overlap the viewport, sorted top-to-bottom by visual position.
  List<_VisibleItemSnapshot> _visibleItemSnapshots() {
    final sliver = _sliver;
    if (sliver == null) return const [];

    final viewportBox = _viewportBox;
    if (viewportBox == null) return const [];

    // Walk every child the sliver currently has laid out.
    final visibleItems = <_VisibleItemSnapshot>[];
    RenderBox? child = sliver.firstChild;
    while (child != null) {
      final parentData = child.parentData;
      if (parentData is SliverMultiBoxAdaptorParentData &&
          child is _RenderMeasuredItem) {
        final index = parentData.index;
        if (index != null && index >= 0) {
          // Read identity from the render object, not from widget.data —
          // the data may already be mutated while the render tree is stale.
          final id = child._id;

          // Compute where this child sits in viewport coordinates.
          final top = _itemTopInViewportBox(child, id);
          if (top != null) {
            final bottom = top + _estimatedHeight(id);

            // Only keep children that overlap the visible area.
            if (bottom > 0.0 && top < viewportBox.size.height) {
              visibleItems.add(
                _VisibleItemSnapshot(id: id, index: index, top: top),
              );
            }
          }
        }
      }
      child = sliver.childAfter(child);
    }

    // Sort top-to-bottom so the caller can pick anchors by position.
    visibleItems.sort((a, b) => a.top.compareTo(b.top));
    return visibleItems;
  }

  /// Sums estimated heights for items 0..<index to approximate a scroll
  /// offset. Uses exact heights when available, falling back to the
  /// cache's smoothed average for unmeasured items.
  double _estimateOffsetAtIndex(int index) {
    var offset = 0.0;
    final items = widget.data.items;
    for (var i = 0; i < index && i < items.length; i++) {
      final id = widget.idExtractor(items[i]);
      offset += _estimatedHeight(id);
    }
    return offset;
  }

  /// Returns the exact measured height if available, otherwise the
  /// cache's smoothed average.
  double _estimatedHeight(Object id) {
    return _heightCache.lookupHeight(id) ?? _heightCache.averageHeight;
  }
}

/// A frozen snapshot of which items are visible and where, captured
/// *before* a data change triggers a rebuild. Used by the layout
/// correction engine to detect drift.
class _ViewportSnapshot {
  const _ViewportSnapshot(this.visibleItems);

  /// Sorted top-to-bottom (visually): first = highest on screen.
  final List<_VisibleItemSnapshot> visibleItems;

  /// Anchor candidates in preference order: bottom-most first, because
  /// in a chat the user's attention is at the bottom.
  Iterable<_VisibleItemSnapshot> get anchorCandidates => visibleItems.reversed;

  /// Index terminology: "lowest" / "highest" refer to list index, not
  /// screen position. In a reversed list, the lowest index (newest
  /// message) is visually at the bottom.
  int get lowestVisibleIndex => visibleItems.last.index;
  int get highestVisibleIndex => visibleItems.first.index;
}

/// One item's identity, list index, and viewport-relative top position
/// at the moment the snapshot was taken.
class _VisibleItemSnapshot {
  const _VisibleItemSnapshot({
    required this.id,
    required this.index,
    required this.top,
  });

  final Object id;
  final int index;
  final double top;
}

/// Bundles a viewport snapshot with the diffs that triggered it, so the
/// correction resolver can check whether diffs touched visible items.
class _LayoutCorrectionRequest<T> {
  const _LayoutCorrectionRequest({required this.snapshot, required this.diffs});

  final _ViewportSnapshot snapshot;
  final List<AnchoredListDiff<T>> diffs;
}

/// Custom [ScrollController] that creates [_AnchoredScrollPosition]
/// instances wired to the layout correction callback.
class _AnchoredScrollController extends ScrollController {
  _AnchoredScrollController({required this.resolveLayoutCorrection});

  final double? Function(ScrollMetrics oldPosition, ScrollMetrics newPosition)
  resolveLayoutCorrection;

  @override
  ScrollPosition createScrollPosition(
    ScrollPhysics physics,
    ScrollContext context,
    ScrollPosition? oldPosition,
  ) {
    return _AnchoredScrollPosition(
      physics: physics,
      context: context,
      initialPixels: initialScrollOffset,
      keepScrollOffset: keepScrollOffset,
      oldPosition: oldPosition,
      debugLabel: debugLabel,
      resolveLayoutCorrection: resolveLayoutCorrection,
    );
  }
}

/// Custom [ScrollPosition] that hooks into [correctForNewDimensions] —
/// the framework's callback between layout passes — to apply scroll
/// offset corrections that keep visible content in place after
/// out-of-viewport mutations.
class _AnchoredScrollPosition extends ScrollPositionWithSingleContext {
  _AnchoredScrollPosition({
    required super.physics,
    required super.context,
    required this.resolveLayoutCorrection,
    super.initialPixels,
    super.keepScrollOffset,
    super.oldPosition,
    super.debugLabel,
  });

  final double? Function(ScrollMetrics oldPosition, ScrollMetrics newPosition)
  resolveLayoutCorrection;

  /// Guards against double-correction within a single layout cycle.
  bool _correctionApplied = false;

  @override
  bool correctForNewDimensions(
    ScrollMetrics oldPosition,
    ScrollMetrics newPosition,
  ) {
    final correctedPixels = resolveLayoutCorrection(oldPosition, newPosition);
    if (correctedPixels != null && (correctedPixels - pixels).abs() >= 0.5) {
      // Apply our correction and return false to request another layout
      // pass so the viewport repaints at the corrected offset.
      correctPixels(correctedPixels);
      _correctionApplied = true;
      return false;
    }
    // On the second pass after our correction, suppress super's default
    // clamping. Our correction already clamped to the current extent;
    // letting super clamp again can oscillate when the extent estimate
    // shifts between passes.
    if (_correctionApplied) {
      _correctionApplied = false;
      return true;
    }
    return super.correctForNewDimensions(oldPosition, newPosition);
  }
}

/// Custom sliver delegate that overrides [estimateMaxScrollOffset] to use
/// the height cache's smoothed average instead of Flutter's default
/// per-layout-pass average. This produces a stable scroll extent even as
/// variable-height items are laid out, preventing scrollbar thumb jitter.
///
/// `addRepaintBoundaries: false` and `addSemanticIndexes: false` are set
/// because the parent widget ([_MeasuredItem]) already wraps each child
/// in a RenderObject, and semantic indexing is not needed for chat items.
class _HeightCachingDelegate extends SliverChildBuilderDelegate {
  _HeightCachingDelegate({
    required NullableIndexedWidgetBuilder builder,
    required int childCount,
    required ChildIndexGetter findChildIndexCallback,
    required this.heightCache,
  }) : super(
         builder,
         childCount: childCount,
         findChildIndexCallback: findChildIndexCallback,
         addRepaintBoundaries: false,
         addSemanticIndexes: false,
       );

  final AnchoredListHeightCache heightCache;

  @override
  double? estimateMaxScrollOffset(
    int firstIndex,
    int lastIndex,
    double leadingScrollOffset,
    double trailingScrollOffset,
  ) {
    final total = childCount;
    if (total == null || total == 0) return null;
    // All items are laid out — no estimation needed.
    if (lastIndex >= total - 1) return trailingScrollOffset;

    final laidOutCount = lastIndex - firstIndex + 1;
    if (laidOutCount <= 0) return null;

    // Use the cache's smoothed average when available; fall back to the
    // current layout pass's average for the first frame before any
    // heights are cached.
    final laidOutExtent = trailingScrollOffset - leadingScrollOffset;
    final avgHeight = heightCache.cachedCount > 0
        ? heightCache.averageHeight
        : laidOutExtent / laidOutCount;

    final remainingCount = total - lastIndex - 1;
    return trailingScrollOffset + remainingCount * avgHeight;
  }
}

/// Wraps a child widget in a [RenderProxyBox] that records the child's
/// height into the [AnchoredListHeightCache] after every layout. This is
/// how the cache stays current — every item self-reports its measured size.
class _MeasuredItem extends SingleChildRenderObjectWidget {
  const _MeasuredItem({
    super.key,
    required this.id,
    required this.cache,
    required Widget child,
  }) : super(child: child);

  final Object id;
  final AnchoredListHeightCache cache;

  @override
  RenderObject createRenderObject(BuildContext context) {
    return _RenderMeasuredItem(id: id, cache: cache);
  }

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderMeasuredItem renderObject,
  ) {
    renderObject
      ..id = id
      ..cache = cache;
  }
}

/// The render object behind [_MeasuredItem]. Delegates all layout to its
/// child via [RenderProxyBox], then writes the resulting height into the
/// [AnchoredListHeightCache]. This runs on every layout pass, so the cache
/// stays current when items resize (e.g. an image finishes loading or a
/// message bubble reflows after a width change).
class _RenderMeasuredItem extends RenderProxyBox {
  _RenderMeasuredItem({
    required Object id,
    required AnchoredListHeightCache cache,
  }) : _id = id,
       _cache = cache;

  Object _id;
  AnchoredListHeightCache _cache;

  set id(Object value) => _id = value;
  set cache(AnchoredListHeightCache value) => _cache = value;

  @override
  void performLayout() {
    super.performLayout();
    if (child != null) {
      _cache.setHeight(_id, size.height);
    }
  }
}
