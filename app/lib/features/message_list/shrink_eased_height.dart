// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

/// Follows [height], easing it down when it shrinks and jumping when it grows.
///
/// When the composer shrinks on send, the list sinks into the freed space at
/// the pace a new row grows into it, so the two cancel out instead of the list
/// jumping down and back up. Growing follows at once, or the composer would
/// cover the newest row for a moment.
class ShrinkEasedHeight extends StatefulWidget {
  const ShrinkEasedHeight({
    super.key,
    required this.height,
    this.hold,
    required this.builder,
  });

  final ValueListenable<double> height;

  /// While true, a shrink waits, so it starts in the frame the sent row
  /// starts growing.
  final ValueListenable<bool>? hold;

  final Widget Function(BuildContext context, double height) builder;

  @override
  State<ShrinkEasedHeight> createState() => _ShrinkEasedHeightState();
}

class _ShrinkEasedHeightState extends State<ShrinkEasedHeight>
    with SingleTickerProviderStateMixin {
  // Same motion as a row arriving in the list.
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: Effect.duration(MotionPreset.short),
  )..addListener(_update);
  late double _from = widget.height.value;
  late double _value = widget.height.value;

  @override
  void initState() {
    super.initState();
    widget.height.addListener(_onHeight);
    widget.hold?.addListener(_onHeight);
  }

  @override
  void didUpdateWidget(ShrinkEasedHeight oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.height != widget.height || oldWidget.hold != widget.hold) {
      oldWidget.height.removeListener(_onHeight);
      oldWidget.hold?.removeListener(_onHeight);
      widget.height.addListener(_onHeight);
      widget.hold?.addListener(_onHeight);
      _onHeight();
    }
  }

  @override
  void dispose() {
    widget.height.removeListener(_onHeight);
    widget.hold?.removeListener(_onHeight);
    _controller.dispose();
    super.dispose();
  }

  void _onHeight() {
    final target = widget.height.value;
    if (target >= _value) {
      _controller.stop();
      setState(() => _value = target);
      return;
    }
    if (widget.hold?.value ?? false) return;
    // Already easing towards it.
    if (_controller.isAnimating) return;
    _from = _value;
    _controller.forward(from: 0);
  }

  void _update() {
    final t = Effect.easeOutQuart.transform(_controller.value);
    setState(() => _value = _from + (widget.height.value - _from) * t);
  }

  @override
  Widget build(BuildContext context) => widget.builder(context, _value);
}
