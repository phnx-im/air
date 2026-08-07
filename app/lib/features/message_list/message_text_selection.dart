// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async' show scheduleMicrotask;

import 'package:air/features/message_list/swipe_to_reply.dart';
import 'package:flutter/cupertino.dart'
    show cupertinoTextSelectionHandleControls;
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

/// The devices the region hands its touch gestures to, and so the ones the
/// gestures taken from it have to cover.
const Set<PointerDeviceKind> _touchDevices = {
  PointerDeviceKind.touch,
  PointerDeviceKind.stylus,
  PointerDeviceKind.invertedStylus,
};

/// How the text inside a message bubble can be selected.
enum MessageSelection {
  /// Not selectable: the copy of a bubble shown inside an overlay, and the
  /// placeholders that stand in for a message rather than carry its text.
  off,

  /// Mouse selection: click and drag, double-click a word, triple-click the
  /// message. The toolbar and handles stay off, the right-click menu offers
  /// the copy.
  pointer,

  /// Touch selection: double-tap a word, triple-tap the message, then the
  /// platform's own handles and toolbar. See [TouchSelectableText].
  touch,
}

/// Message text a finger can select, without taking the gestures the bubble
/// already carries.
///
/// [SelectableRegion] alone doesn't do on touch what the bubble needs:
///
///  * it claims a long press for its own word selection, where the message
///    actions have to open instead, and
///  * it caps a touch series below the third tap (Flutter only selects a
///    paragraph on a triple tap from a precise pointer), leaving no way to take
///    the whole message.
///
/// So the long press is taken here, ahead of the region, and the third tap
/// selects everything the region holds -- which is exactly one message. The
/// horizontal drag the region claims for its own selection (and then drops,
/// since it only drags a selection under a precise pointer) goes back to
/// swipe-to-reply the same way.
class TouchSelectableText extends StatefulWidget {
  const TouchSelectableText({
    super.key,
    required this.onLongPress,
    required this.child,
  });

  /// Opens the message actions, in place of the region's word selection.
  ///
  /// Registering no callback hands the long press back to the region.
  final VoidCallback? onLongPress;

  final Widget child;

  @override
  State<TouchSelectableText> createState() => _TouchSelectableTextState();
}

class _TouchSelectableTextState extends State<TouchSelectableText> {
  final GlobalKey<SelectableRegionState> _regionKey = GlobalKey();

  /// Taps of the series in progress, and the down that started the last one.
  /// Taken from the raw pointer stream rather than from a recognizer, so the
  /// region keeps seeing the first two taps and selects the word itself.
  int _tapCount = 0;
  PointerDownEvent? _lastTapDown;

  void _handlePointerDown(PointerDownEvent event) {
    final last = _lastTapDown;
    final continues =
        last != null &&
        event.timeStamp - last.timeStamp <= kDoubleTapTimeout &&
        (event.position - last.position).distance <= kDoubleTapSlop;
    _tapCount = continues ? _tapCount + 1 : 1;
    _lastTapDown = event;
  }

  void _handlePointerUp(PointerUpEvent event) {
    final down = _lastTapDown;
    // A drag ends the series rather than continuing it.
    if (down != null &&
        (event.position - down.position).distance > kTouchSlop) {
      _tapCount = 0;
      _lastTapDown = null;
      return;
    }
    if (_tapCount != 3) return;
    // The region handles this tap first -- on Android its series has already
    // wrapped around to a single tap, which collapses what the double tap
    // selected -- so the message-wide selection only holds once the event is
    // through.
    scheduleMicrotask(_selectMessage);
  }

  void _handlePointerCancel(PointerCancelEvent event) {
    _tapCount = 0;
    _lastTapDown = null;
  }

  void _selectMessage() {
    // The cause the region takes as "raise the handles and the toolbar".
    _regionKey.currentState?.selectAll(SelectionChangedCause.toolbar);
  }

  @override
  Widget build(BuildContext context) {
    final swipe = SwipeToReplyScope.maybeOf(context);
    return SelectableRegion(
      key: _regionKey,
      selectionControls: switch (Theme.of(context).platform) {
        TargetPlatform.iOS => cupertinoTextSelectionHandleControls,
        _ => materialTextSelectionHandleControls,
      },
      contextMenuBuilder: (context, state) =>
          AdaptiveTextSelectionToolbar.selectableRegion(
            selectableRegionState: state,
          ),
      magnifierConfiguration: TextMagnifier.adaptiveMagnifierConfiguration,
      child: Listener(
        onPointerDown: _handlePointerDown,
        onPointerUp: _handlePointerUp,
        onPointerCancel: _handlePointerCancel,
        // Sits under the region's own detector, so it enters the gesture arena
        // first and its long press resolves ahead of the region's.
        child: RawGestureDetector(
          behavior: HitTestBehavior.translucent,
          excludeFromSemantics: true,
          gestures: {
            LongPressGestureRecognizer:
                GestureRecognizerFactoryWithHandlers<
                  LongPressGestureRecognizer
                >(
                  () => LongPressGestureRecognizer(
                    debugOwner: this,
                    supportedDevices: _touchDevices,
                  ),
                  (instance) => instance.onLongPress = widget.onLongPress,
                ),
            if (swipe != null)
              HorizontalDragGestureRecognizer:
                  GestureRecognizerFactoryWithHandlers<
                    HorizontalDragGestureRecognizer
                  >(
                    () => HorizontalDragGestureRecognizer(
                      debugOwner: this,
                      supportedDevices: _touchDevices,
                    ),
                    (instance) => instance
                      ..onStart = swipe.dragStart
                      ..onUpdate = swipe.dragUpdate
                      ..onEnd = swipe.dragEnd
                      ..onCancel = swipe.dragCancel,
                  ),
          },
          child: widget.child,
        ),
      ),
    );
  }
}
