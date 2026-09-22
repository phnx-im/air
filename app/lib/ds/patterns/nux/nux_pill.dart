// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// A splash screen's top-edge pill: a boxed glyph, then a label.
class NuxPill extends StatelessWidget {
  const NuxPill({
    super.key,
    required this.icon,
    required this.label,
    this.onTap,
  });

  final AppIconType icon;
  final String label;
  final VoidCallback? onTap;

  static const double _glyphBox = S.s32;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    final content = Padding(
      padding: const EdgeInsets.only(right: S.s16),
      child: Row(
        mainAxisSize: .min,
        children: [
          SizedBox.square(
            dimension: _glyphBox,
            child: Center(
              child: AppIcon(
                type: icon,
                color: palette.text.primary,
                size: S.s16,
              ),
            ),
          ),
          const SizedBox(width: S.s8),
          Text(
            label,
            style: typeScale.body.s.style(color: palette.text.primary),
          ),
        ],
      ),
    );

    final fill = DecoratedBox(
      decoration: BoxDecoration(
        color: palette.fill.tertiary,
        borderRadius: BorderRadius.circular(CornerRadius.full),
      ),
    );

    if (onTap == null) {
      return Stack(
        children: [
          Positioned.fill(child: fill),
          content,
        ],
      );
    }

    return StateLayer(
      borderRadius: CornerRadius.full,
      surface: palette.fill.tertiary,
      onTap: onTap,
      background: fill,
      child: content,
    );
  }
}
