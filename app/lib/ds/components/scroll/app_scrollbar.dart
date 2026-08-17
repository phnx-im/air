// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/components/scroll/app_scrollbar_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/util/frame.dart';
import 'package:flutter/widgets.dart';

/// Overlays [child] with a scrollbar thumb that shows while the content moves
/// and fades out once it settles.
///
/// Tracks whatever vertical scrollable sits directly inside [child] through its
/// notifications, so the host keeps its own controller and doesn't have to
/// hand one over. The platform's own scrollbar has to be off in [child], or
/// both show at once.
///
/// [trackTop] and [trackBottom] shorten the track from each end so it clears
/// chrome floating over the scrollable, such as a header bed or a composer.
class AppScrollbar extends StatefulWidget {
  const AppScrollbar({
    super.key,
    required this.child,
    this.trackTop = 0,
    this.trackBottom = 0,
  });

  final Widget child;
  final double trackTop;
  final double trackBottom;

  @override
  State<AppScrollbar> createState() => _AppScrollbarState();
}

class _AppScrollbarState extends State<AppScrollbar> with FrameSafeState {
  /// We hold it in a notifier rather than in state so a scroll repaints the
  /// thumb alone and never rebuilds the list behind it.
  final _thumb = ValueNotifier<_ThumbState>(_ThumbState.hidden);

  Timer? _hideTimer;

  @override
  void dispose() {
    _hideTimer?.cancel();
    _thumb.dispose();
    super.dispose();
  }

  /// Metrics arriving without a scroll -- the initial layout, or rows added
  /// while the list sits still -- resize the thumb but must not flash it in.
  bool _onMetrics(ScrollMetricsNotification notification) {
    _track(notification.depth, notification.metrics, scrolling: false);
    return false;
  }

  bool _onScroll(ScrollNotification notification) {
    _track(notification.depth, notification.metrics, scrolling: !isMidFrame);
    return false;
  }

  void _track(int depth, ScrollMetrics metrics, {required bool scrolling}) {
    // A scrollable nested inside a row reports through here too, at a depth
    // below the one this bar stands for.
    if (depth != 0 || metrics.axis != Axis.vertical) return;

    final next = _ThumbState.of(
      metrics,
      visible: scrolling || _thumb.value.visible,
    );
    runFrameSafe(() => _thumb.value = next);
    if (!scrolling) return;

    _hideTimer?.cancel();
    _hideTimer = Timer(AppScrollbarTokens.hideDelay, _hide);
  }

  void _hide() {
    if (!mounted) return;
    _thumb.value = _thumb.value.concealed;
  }

  @override
  Widget build(BuildContext context) {
    final tokens = AppScrollbarTokens.current;
    final color = SemanticPalette.of(context).text.quaternary;

    return NotificationListener<ScrollMetricsNotification>(
      onNotification: _onMetrics,
      child: NotificationListener<ScrollNotification>(
        onNotification: _onScroll,
        child: Stack(
          // The host sizes the bar, exactly as it sized the scrollable before
          // the bar wrapped it, and its chrome may deliberately spill out.
          fit: StackFit.passthrough,
          clipBehavior: Clip.none,
          children: [
            widget.child,
            Positioned.fill(
              child: IgnorePointer(
                child: ValueListenableBuilder<_ThumbState>(
                  valueListenable: _thumb,
                  builder: (context, thumb, _) => _Thumb(
                    thumb: thumb,
                    tokens: tokens,
                    color: color,
                    trackTop: widget.trackTop,
                    trackBottom: widget.trackBottom,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Thumb extends StatelessWidget {
  const _Thumb({
    required this.thumb,
    required this.tokens,
    required this.color,
    required this.trackTop,
    required this.trackBottom,
  });

  final _ThumbState thumb;
  final AppScrollbarTokens tokens;
  final Color color;
  final double trackTop;
  final double trackBottom;

  @override
  Widget build(BuildContext context) {
    // Content that fits the viewport has nowhere to scroll, so there's no
    // position to mark.
    if (thumb.extentRatio >= 1) return const SizedBox.shrink();

    final top = trackTop + AppScrollbarTokens.trackInset;
    final bottom = trackBottom + AppScrollbarTokens.trackInset;
    final trackHeight = thumb.viewportHeight - top - bottom;
    if (trackHeight <= 0) return const SizedBox.shrink();

    final height = (thumb.extentRatio * trackHeight).clamp(
      tokens.minThumbHeight,
      trackHeight,
    );

    return Align(
      alignment: Alignment.topRight,
      child: Padding(
        padding: EdgeInsets.only(
          top: top + thumb.offsetRatio * (trackHeight - height),
          right: AppScrollbarTokens.rightInset,
        ),
        child: AnimatedOpacity(
          opacity: thumb.visible ? 1 : 0,
          duration: thumb.visible
              ? Duration.zero
              : Effect.duration(AppScrollbarTokens.hideMotion),
          child: Container(
            width: AppScrollbarTokens.width,
            height: height,
            decoration: BoxDecoration(
              color: color,
              borderRadius: BorderRadius.circular(AppScrollbarTokens.radius),
            ),
          ),
        ),
      ),
    );
  }
}

/// Where the thumb sits and how much of the track it covers, as fractions the
/// track's own height resolves against.
@immutable
class _ThumbState {
  const _ThumbState({
    required this.offsetRatio,
    required this.extentRatio,
    required this.viewportHeight,
    required this.visible,
  });

  factory _ThumbState.of(ScrollMetrics metrics, {required bool visible}) {
    final range = metrics.maxScrollExtent - metrics.minScrollExtent;
    final viewport = metrics.viewportDimension;
    final content = viewport + range;
    final progress = range > 0
        ? ((metrics.pixels - metrics.minScrollExtent) / range).clamp(0.0, 1.0)
        : 0.0;
    return _ThumbState(
      // A reversed list counts its offset up from the bottom, while the thumb
      // still runs down the track.
      offsetRatio: axisDirectionIsReversed(metrics.axisDirection)
          ? 1 - progress
          : progress,
      extentRatio: content > 0 ? (viewport / content).clamp(0.0, 1.0) : 1.0,
      viewportHeight: viewport,
      visible: visible,
    );
  }

  /// A list that hasn't reported any metrics yet.
  static const _ThumbState hidden = _ThumbState(
    offsetRatio: 0,
    extentRatio: 1,
    viewportHeight: 0,
    visible: false,
  );

  final double offsetRatio;
  final double extentRatio;
  final double viewportHeight;
  final bool visible;

  _ThumbState get concealed => _ThumbState(
    offsetRatio: offsetRatio,
    extentRatio: extentRatio,
    viewportHeight: viewportHeight,
    visible: false,
  );

  @override
  bool operator ==(Object other) =>
      other is _ThumbState &&
      other.offsetRatio == offsetRatio &&
      other.extentRatio == extentRatio &&
      other.viewportHeight == viewportHeight &&
      other.visible == visible;

  @override
  int get hashCode =>
      Object.hash(offsetRatio, extentRatio, viewportHeight, visible);
}
