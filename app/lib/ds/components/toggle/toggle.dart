// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/toggle/toggle_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Switch-style toggle: a pill track whose thumb slides between the two ends
/// as the track changes color.
///
/// Carries no label. A caller that needs one pairs it with its own text and
/// puts the tap handler on the whole row.
///
/// Unlike the other interactive surfaces this one takes no hover or press
/// wash. The track and the thumb are the state, so a wash on top would read as
/// a third value, and the thumb's travel is already the feedback.
class Toggle extends StatelessWidget {
  const Toggle({
    super.key,
    required this.tokens,
    required this.value,
    this.onChanged,
    this.enabled = true,
  });

  final ToggleTokens tokens;
  final bool value;

  /// Reports the value the toggle would flip to. Null leaves the track inert
  /// and dimmed, same as `enabled: false`.
  final ValueChanged<bool>? onChanged;

  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final interactive = enabled && onChanged != null;
    final duration = Effect.duration(ToggleTokens.motion);

    final track = value ? palette.accentBrand.primary : palette.fill.tertiary;
    final thumbLeft = value
        ? tokens.trackWidth - tokens.thumbSize - ToggleTokens.thumbPadding
        : ToggleTokens.thumbPadding;

    return Opacity(
      opacity: interactive ? Alpha.a100 : ToggleTokens.disabledAlpha,
      child: MouseRegion(
        cursor: interactive ? SystemMouseCursors.click : MouseCursor.defer,
        child: GestureDetector(
          onTap: interactive ? () => onChanged!(!value) : null,
          child: AnimatedContainer(
            duration: duration,
            curve: Effect.easeOutQuart,
            width: tokens.trackWidth,
            height: tokens.trackHeight,
            decoration: BoxDecoration(
              color: track,
              borderRadius: BorderRadius.circular(CornerRadius.full),
            ),
            child: Stack(
              children: [
                AnimatedPositioned(
                  duration: duration,
                  curve: Effect.easeOutQuart,
                  top: ToggleTokens.thumbPadding,
                  left: thumbLeft,
                  child: Container(
                    width: tokens.thumbSize,
                    height: tokens.thumbSize,
                    decoration: BoxDecoration(
                      color: value
                          ? palette.accentBrand.onPrimary
                          : palette.function.neutral.toggleWhite,
                      shape: .circle,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
