// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:flutter/widgets.dart';

/// The message input: a round leading button, the field, and a trailing button
/// that's either send or scroll-back.
///
/// The three elements wear the header chrome, an opaque elevated fill on a flat
/// shadow, so the input reads as the same kind of floating control as the bar
/// at the other end of the conversation.
///
/// A pure view: it renders what it's handed and reports gestures back, so the
/// host owns the text, the draft, and the scroll.
class MessageInput extends StatefulWidget {
  const MessageInput({
    super.key,
    required this.tokens,
    required this.field,
    this.aboveField = const [],
    required this.leadingIcon,
    this.onLeading,
    required this.sendIcon,
    this.showSend = false,
    this.onSend,
    this.showScrollBack = false,
    this.scrollBackUnread = false,
    this.onScrollBack,
  });

  final MessageInputTokens tokens;

  /// The text field. Renders inside the field's chrome, so it carries no
  /// decoration and no content padding of its own, the pattern insets it.
  final Widget field;

  /// Rows stacked above the field inside the same chrome, an edit banner or a
  /// reply preview. They take the field's horizontal inset and own their
  /// vertical spacing.
  final List<Widget> aboveField;

  /// Attach, or cancel while editing.
  final AppIconType leadingIcon;

  /// Handed the button's own context, so a menu opened from here anchors to
  /// the button rather than to whatever the host happens to build it under.
  final void Function(BuildContext buttonContext)? onLeading;

  /// Send, or confirm while editing.
  final AppIconType sendIcon;

  /// Whether there's anything to send. The host decides what counts, the
  /// pattern only animates the slot.
  final bool showSend;

  final VoidCallback? onSend;

  /// Show the scroll-back affordance while the reader is scrolled up. Send
  /// always wins the slot the moment there's something to send.
  final bool showScrollBack;

  /// Pin the unread dot to the scroll-back button, to signal messages the
  /// reader hasn't seen below the fold.
  final bool scrollBackUnread;

  final VoidCallback? onScrollBack;

  @override
  State<MessageInput> createState() => _MessageInputState();
}

class _MessageInputState extends State<MessageInput>
    with TickerProviderStateMixin {
  late final AnimationController _sendAnim;
  late final Animation<double> _sendCurve;
  late final AnimationController _scrollBackAnim;
  late final Animation<double> _scrollBackCurve;

  @override
  void initState() {
    super.initState();
    _sendAnim = AnimationController(
      vsync: this,
      duration: MessageInputTokens.sendMotion.duration,
      value: widget.showSend ? 1.0 : 0.0,
    );
    _sendCurve = CurvedAnimation(parent: _sendAnim, curve: Effect.easeOutQuart);
    _scrollBackAnim = AnimationController(
      vsync: this,
      duration: MessageInputTokens.scrollBackMotion.duration,
      value: _scrollBackVisible ? 1.0 : 0.0,
    );
    _scrollBackCurve = CurvedAnimation(
      parent: _scrollBackAnim,
      curve: Effect.easeOutQuart,
    );
  }

  @override
  void didUpdateWidget(MessageInput oldWidget) {
    super.didUpdateWidget(oldWidget);
    _drive();
  }

  @override
  void dispose() {
    _sendAnim.dispose();
    _scrollBackAnim.dispose();
    super.dispose();
  }

  /// Scroll-back yields the slot to send rather than the two sharing it.
  bool get _scrollBackVisible => widget.showScrollBack && !widget.showSend;

  void _drive() {
    if (widget.showSend) {
      _sendAnim.forward();
    } else {
      _sendAnim.reverse();
    }
    if (_scrollBackVisible) {
      _scrollBackAnim.forward();
    } else {
      _scrollBackAnim.reverse();
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = widget.tokens;
    final palette = SemanticPalette.of(context);
    final insetY = t.fieldInsetY;

    return Padding(
      padding: t.containerPadding,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          // Builder so the handler receives the button's context and not this
          // one, which the row above has already laid out and padded.
          Builder(
            builder: (buttonContext) {
              final onLeading = widget.onLeading;
              return _button(
                icon: widget.leadingIcon,
                fill: palette.backgroundElevated.primary,
                onPressed: onLeading == null
                    ? null
                    : () => onLeading(buttonContext),
              );
            },
          ),
          SizedBox(width: t.gap),
          Expanded(
            child: Container(
              constraints: BoxConstraints(minHeight: t.buttonSize),
              decoration: BoxDecoration(
                color: palette.backgroundElevated.primary,
                borderRadius: BorderRadius.circular(t.inputRadius),
                boxShadow: Effect.elevation(Elevation.flat),
              ),
              padding: EdgeInsets.symmetric(horizontal: t.fieldPadding),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                mainAxisSize: MainAxisSize.min,
                children: [
                  ...widget.aboveField,
                  Padding(
                    // The rows above already space themselves off the field's
                    // top edge, so the field only keeps its bottom inset.
                    padding: EdgeInsets.only(
                      top: widget.aboveField.isEmpty ? insetY : 0,
                      bottom: insetY,
                    ),
                    child: widget.field,
                  ),
                ],
              ),
            ),
          ),
          _RevealSlot(
            sizeFactor: _scrollBackCurve,
            gap: t.gap,
            enterScale: MessageInputTokens.scrollBackEnterScale,
            child: _UnreadDot(
              show: widget.scrollBackUnread,
              child: _button(
                icon: AppIconType.chevronDown,
                fill: palette.backgroundElevated.primary,
                onPressed: widget.onScrollBack,
              ),
            ),
          ),
          _RevealSlot(
            sizeFactor: _sendCurve,
            gap: t.gap,
            enterScale: MessageInputTokens.sendEnterScale,
            // Send is the one element that breaks the row's neutral chrome:
            // inverted, so the primary action reads at a glance.
            child: _button(
              icon: widget.sendIcon,
              fill: palette.function.neutral.toggleBlack,
              iconColor: palette.function.neutral.toggleWhite,
              onPressed: widget.onSend,
            ),
          ),
        ],
      ),
    );
  }

  Widget _button({
    required AppIconType icon,
    required Color fill,
    Color? iconColor,
    VoidCallback? onPressed,
  }) => ButtonIcon(
    variant: ButtonIconVariant.solid,
    icon: icon,
    size: widget.tokens.buttonSize,
    fill: fill,
    iconColor: iconColor,
    shadows: Effect.elevation(Elevation.flat),
    onPressed: onPressed,
  );
}

/// Reveals a trailing button by reflowing the field open as [sizeFactor] runs
/// 0 -> 1, right-anchored so the button slides out from the field's edge, while
/// it fades and grows from [enterScale]. Always mounted, so it animates out the
/// same way it came in.
///
/// This is `SizeTransition(axis: horizontal)` minus its `ClipRect`: that clip
/// is tight to the slot's width, which crops the button's shadow at the input's
/// right edge. Unclipped, the shadow paints in full, and the button only bleeds
/// over the field mid-transition, while it's still faded out.
class _RevealSlot extends AnimatedWidget {
  const _RevealSlot({
    required Animation<double> sizeFactor,
    required this.gap,
    required this.enterScale,
    required this.child,
  }) : super(listenable: sizeFactor);

  final double gap;
  final double enterScale;
  final Widget child;

  Animation<double> get _sizeFactor => listenable as Animation<double>;

  @override
  Widget build(BuildContext context) {
    final t = _sizeFactor.value;
    if (t == 0) return const SizedBox.shrink();
    return Align(
      // What SizeTransition's deprecated `axisAlignment: 1.0` resolved to.
      alignment: const AlignmentDirectional(1.0, -1.0),
      widthFactor: t.clamp(0.0, double.infinity),
      child: Padding(
        padding: EdgeInsets.only(left: gap),
        child: Opacity(
          opacity: t.clamp(0.0, 1.0),
          child: Transform.scale(
            scale: enterScale + (1.0 - enterScale) * t,
            child: child,
          ),
        ),
      ),
    );
  }
}

/// Pins a small circular badge to the top-right corner of [child] when [show]
/// is true, and returns [child] untouched otherwise.
class _UnreadDot extends StatelessWidget {
  const _UnreadDot({required this.show, required this.child});

  final bool show;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    if (!show) return child;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        child,
        Positioned(
          top: MessageInputTokens.dotInsetTop,
          right: MessageInputTokens.dotInsetRight,
          child: Container(
            width: MessageInputTokens.dotSize,
            height: MessageInputTokens.dotSize,
            decoration: BoxDecoration(
              color: SemanticPalette.of(context).function.neutral.toggleBlack,
              shape: BoxShape.circle,
            ),
          ),
        ),
      ],
    );
  }
}
