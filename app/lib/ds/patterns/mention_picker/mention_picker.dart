// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/mention_picker/mention_picker_tokens.dart';
import 'package:flutter/widgets.dart';

/// One member in the mention picker: avatar and name.
///
/// A pure view: the host floats the rows in a card styled by
/// [MentionPickerTokens], and filters, sorts and caps the members.
class MentionPickerRow extends StatelessWidget {
  const MentionPickerRow({
    super.key,
    required this.tokens,
    required this.avatar,
    required this.name,
    this.highlighted = false,
  });

  final MentionPickerTokens tokens;

  /// The host sizes it to [MentionPickerTokens.avatarSize].
  final Widget avatar;

  final String name;
  final bool highlighted;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Container(
      padding: tokens.rowPadding,
      decoration: BoxDecoration(
        color: highlighted ? MentionPickerTokens.highlightFill(palette) : null,
        borderRadius: BorderRadius.circular(MentionPickerTokens.rowRadius),
      ),
      child: Row(
        spacing: MentionPickerTokens.avatarGap,
        children: [
          avatar,
          Expanded(
            child: Text(
              name,
              style: typeScale.body.regular.style(color: palette.text.primary),
              maxLines: 1,
              overflow: .ellipsis,
            ),
          ),
        ],
      ),
    );
  }
}
