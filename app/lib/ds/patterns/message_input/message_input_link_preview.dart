// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/link_preview/link_preview.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:flutter/widgets.dart';

/// The link preview staged above the field, with a floating remove button.
///
/// Shows the pending card while [data] is null, and no remove button then,
/// since there is nothing to drop yet.
class MessageInputLinkPreview extends StatelessWidget {
  const MessageInputLinkPreview({
    super.key,
    required this.url,
    this.data,
    this.imageBuilder,
    this.onRemove,
  });

  /// The address, shown on the pending card.
  final String url;

  /// The resolved card, null while it is being fetched.
  final LinkPreviewData? data;

  /// Builds the card's image, see [LinkPreview.imageBuilder].
  final Widget Function(BoxFit fit)? imageBuilder;

  /// Unstages the preview.
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final onRemove = this.onRemove;

    return Padding(
      padding: const EdgeInsets.only(
        left: MessageInputLinkPreviewTokens.gapSide,
        right: MessageInputLinkPreviewTokens.gapSide,
        top: MessageInputLinkPreviewTokens.gapAbove,
        bottom: MessageInputLinkPreviewTokens.gapBelow,
      ),
      child: Stack(
        // The remove button's ring reaches past the card and must not be
        // cropped to it.
        clipBehavior: .none,
        children: [
          LinkPreview(url: url, data: data, imageBuilder: imageBuilder),
          if (onRemove != null && data != null)
            Positioned(
              top: MessageInputLinkPreviewTokens.removeOffset,
              right: MessageInputLinkPreviewTokens.removeOffset,
              child: ButtonIcon(
                variant: ButtonIconVariant.solid,
                icon: AppIconType.x,
                size: MessageInputLinkPreviewTokens.removeSize,
                iconSize: MessageInputLinkPreviewTokens.removeIconSize,
                fill: palette.backgroundElevated.primary,
                hitTargetSize: MessageInputLinkPreviewTokens.removeHitTarget,
                onPressed: onRemove,
              ),
            ),
        ],
      ),
    );
  }
}
