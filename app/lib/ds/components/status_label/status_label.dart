// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/status_label/status_label_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// A short state line under a name, such as a contact's verification.
///
/// A pure view with a small glyph and a word or two. The text style comes from
/// [tokens] and the color from [StatusLabelTokens.color].
class StatusLabel extends StatelessWidget {
  const StatusLabel({
    super.key,
    required this.tokens,
    required this.label,
    this.icon,
  });

  final StatusLabelTokens tokens;

  /// Already localized. The component never resolves copy itself.
  final String label;

  /// Leading glyph. Omit for a text-only line.
  final AppIconType? icon;

  @override
  Widget build(BuildContext context) {
    final color = StatusLabelTokens.color(SemanticPalette.of(context));
    final iconType = icon;
    return Row(
      mainAxisSize: .min,
      children: [
        if (iconType != null) ...[
          AppIcon(
            type: iconType,
            size: StatusLabelTokens.iconSize,
            color: color,
          ),
          const SizedBox(width: StatusLabelTokens.iconGap),
        ],
        Flexible(
          child: Text(
            label,
            maxLines: 1,
            overflow: .ellipsis,
            style: tokens.text.style(color: color, tight: tokens.tight),
          ),
        ),
      ],
    );
  }
}
