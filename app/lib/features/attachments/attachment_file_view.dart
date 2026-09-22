// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/widgets.dart';

/// A file attachment the way a message bubble carries it: a glyph, the file
/// name, and its size below.
///
/// The glyph is a slot, so an attachment that has been sent hangs its transfer
/// status off it while one that is only being previewed shows a plain icon.
class AttachmentFileView extends StatelessWidget {
  const AttachmentFileView({
    super.key,
    required this.leading,
    required this.filename,
    required this.size,
    required this.color,
    this.maxLines,
  });

  /// The glyph left of the name.
  final Widget leading;

  final String filename;

  /// File size in bytes.
  final int size;

  /// Color of both lines, taken from the surface the file sits on.
  final Color color;

  /// Lines the name may take. Unset it wraps, which is what a bubble growing
  /// with its content wants. A preview of a fixed height caps it instead.
  final int? maxLines;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Row(
      mainAxisSize: .min,
      spacing: S.s16,
      children: [
        leading,
        // Flexible is needed to make the text wrap if the filename is too long
        Flexible(
          fit: .loose,
          child: Column(
            crossAxisAlignment: .start,
            children: [
              Text(
                filename,
                maxLines: maxLines,
                overflow: maxLines == null ? null : TextOverflow.ellipsis,
                style: typeScale.body.regular.style(color: color),
              ),
              Text(
                loc.bytesToHumanReadable(size),
                style: typeScale.body.xs.style(color: color),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
