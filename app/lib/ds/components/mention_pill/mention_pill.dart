// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/mention_pill/mention_pill_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// Inline pill naming a mentioned member, in message text and in the composer.
///
/// A pure view: the host resolves [name] to the reader's own name for the
/// member, never the sender's typed text.
class MentionPill extends StatelessWidget {
  const MentionPill({
    super.key,
    required this.name,
    this.variant = MentionPillVariant.other,
    this.onTap,
  });

  final String name;
  final MentionPillVariant variant;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final pill = Container(
      padding: MentionPillTokens.padding,
      decoration: BoxDecoration(
        color: MentionPillTokens.fill(palette, variant),
        borderRadius: BorderRadius.circular(MentionPillTokens.radius),
      ),
      child: Text(
        '@$name',
        style: typeScale.body.regular.style(
          color: MentionPillTokens.label(palette, variant),
        ),
      ),
    );
    final onTap = this.onTap;
    return Padding(
      padding: MentionPillTokens.margin,
      child: onTap == null
          ? pill
          : GestureDetector(behavior: .opaque, onTap: onTap, child: pill),
    );
  }
}
