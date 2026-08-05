// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/components/delivery_status/delivery_status_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// How far an own message has travelled.
enum MessageDeliveryStatus { sending, sent, delivered, read, failed }

/// The glyph reporting how far an own message got: one glyph per state,
/// crossfading as the message advances, and a failure in the alarm ink.
///
/// Holds the in-flight delay, so a send that lands right away never flashes a
/// spinner. Through the delay the glyph either collapses or keeps its
/// footprint, see [holdsSpace].
///
/// A pure view. A reader with read receipts off maps [MessageDeliveryStatus]
/// `read` down to `delivered` before passing it: the glyph must not report back
/// more than the setting does.
class DeliveryStatus extends StatefulWidget {
  const DeliveryStatus({
    super.key,
    required this.status,
    required this.size,
    this.holdsSpace = false,
    this.builder,
  });

  final MessageDeliveryStatus status;

  /// Glyph size, from the host's own bundle: a stamp under a bubble reports
  /// quietly, a list gutter carries the row's only glyph and reads at a glance.
  final double size;

  /// Whether an in-flight send keeps the glyph's footprint through the delay.
  /// A gutter beside text holds it, so the text doesn't shift when the spinner
  /// arrives. A stamp that reflows anyway collapses instead.
  final bool holdsSpace;

  /// Places the glyph among whatever the host sets around it -- a separator
  /// before it, a label after it -- in the state's own ink, so the host's parts
  /// pick up the failure color without restating the rule.
  ///
  /// We skip it entirely while a collapsed in-flight send waits out its delay,
  /// so a separator never hangs there on its own.
  final Widget Function(BuildContext context, Widget glyph, Color ink)? builder;

  @override
  State<DeliveryStatus> createState() => _DeliveryStatusState();
}

class _DeliveryStatusState extends State<DeliveryStatus> {
  Timer? _timer;
  bool _inFlightVisible = false;

  @override
  void initState() {
    super.initState();
    _trackInFlight();
  }

  @override
  void didUpdateWidget(DeliveryStatus oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.status != widget.status) {
      _trackInFlight();
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  void _trackInFlight() {
    if (widget.status != MessageDeliveryStatus.sending) {
      _timer?.cancel();
      _timer = null;
      _inFlightVisible = false;
      return;
    }
    if (_inFlightVisible || _timer != null) return;
    _timer = Timer(DeliveryStatusTokens.sendingReveal, () {
      _timer = null;
      setState(() => _inFlightVisible = true);
    });
  }

  @override
  Widget build(BuildContext context) {
    final waiting =
        widget.status == MessageDeliveryStatus.sending && !_inFlightVisible;
    if (waiting && !widget.holdsSpace) return const SizedBox.shrink();

    final palette = SemanticPalette.of(context);
    final ink = widget.status == MessageDeliveryStatus.failed
        ? palette.function.danger
        : palette.text.tertiary;

    final glyph = AnimatedSwitcher(
      duration: Effect.duration(DeliveryStatusTokens.motion),
      child: waiting
          ? SizedBox.square(
              key: const ValueKey('inFlightReserve'),
              dimension: widget.size,
            )
          : _StatusGlyph(
              key: ValueKey(widget.status),
              status: widget.status,
              size: widget.size,
              color: ink,
            ),
    );

    return widget.builder?.call(context, glyph, ink) ?? glyph;
  }
}

class _StatusGlyph extends StatelessWidget {
  const _StatusGlyph({
    super.key,
    required this.status,
    required this.size,
    required this.color,
  });

  final MessageDeliveryStatus status;
  final double size;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return switch (status) {
      MessageDeliveryStatus.sending => _InFlightSpinner(
        size: size,
        color: color,
      ),
      MessageDeliveryStatus.sent => AppIcon.check(size: size, color: color),
      MessageDeliveryStatus.delivered => AppIcon.checkCheck(
        size: size,
        color: color,
      ),
      MessageDeliveryStatus.read => AppIcon.checkCheckFill(
        size: size,
        color: color,
      ),
      MessageDeliveryStatus.failed => AppIcon.circleAlert(
        size: size,
        color: color,
      ),
    };
  }
}

class _InFlightSpinner extends StatefulWidget {
  const _InFlightSpinner({required this.size, required this.color});

  final double size;
  final Color color;

  @override
  State<_InFlightSpinner> createState() => _InFlightSpinnerState();
}

class _InFlightSpinnerState extends State<_InFlightSpinner>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    duration: DeliveryStatusTokens.spinnerPeriod,
    vsync: this,
  )..repeat();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RotationTransition(
      turns: _controller,
      child: AppIcon.circleDashed(size: widget.size, color: widget.color),
    );
  }
}
