// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/checkbox/checkbox_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Square checkbox: an outline while unchecked, a filled box carrying a check
/// glyph once checked.
///
/// Carries no label. A caller that needs one pairs it with its own text and
/// puts the tap handler on the whole row, which also gives the 20px box a
/// full-size target. Hover / press / focus route through the shared
/// [StateLayer], shaped by the platform -- touch dips, a pointer washes.
class AppCheckbox extends StatelessWidget {
  const AppCheckbox({
    super.key,
    required this.value,
    this.onChanged,
    this.enabled = true,
  });

  final bool value;

  /// Reports the value the box would flip to. Null leaves the box inert and
  /// dimmed, same as `enabled: false`.
  final ValueChanged<bool>? onChanged;

  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final interactive = enabled && onChanged != null;
    final duration = Effect.duration(CheckboxTokens.motion);

    Color dim(Color color, double tier) =>
        interactive ? color : color.withValues(alpha: color.a * tier);

    final fill = dim(
      palette.function.neutral.toggleBlack,
      CheckboxTokens.disabledFillAlpha,
    );
    final border = dim(
      palette.text.secondary,
      CheckboxTokens.disabledBorderAlpha,
    );
    final check = dim(
      palette.function.neutral.toggleWhite,
      CheckboxTokens.disabledCheckAlpha,
    );

    return StateLayer(
      borderRadius: CheckboxTokens.radius,
      // The wash sits on the box once it's filled, on the page base while the
      // box is only an outline and its footprint shows through.
      surface: value ? fill : palette.backgroundBase.primary,
      enabled: enabled,
      onTap: onChanged == null ? null : () => onChanged!(!value),
      background: AnimatedContainer(
        duration: duration,
        curve: Effect.easeOutQuart,
        decoration: BoxDecoration(
          color: value ? fill : null,
          borderRadius: BorderRadius.circular(CheckboxTokens.radius),
          border: value
              ? null
              : Border.all(color: border, width: CheckboxTokens.borderWidth),
        ),
      ),
      child: SizedBox(
        width: CheckboxTokens.size,
        height: CheckboxTokens.size,
        child: Center(
          // The glyph grows out of the box rather than appearing whole, so the
          // fill and the check land together.
          child: AnimatedScale(
            scale: value ? 1.0 : 0.0,
            duration: duration,
            curve: Effect.easeOutQuart,
            child: AnimatedOpacity(
              opacity: value ? Alpha.a100 : Alpha.a0,
              duration: duration,
              curve: Effect.easeOutQuart,
              child: AppIcon(
                type: AppIconType.check,
                size: CheckboxTokens.checkSize,
                color: check,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
