// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/patterns/link_preview/link_preview.dart';

extension UiLinkPreviewKindX on UiLinkPreviewKind {
  LinkPreviewKind toDs() => switch (this) {
    .default_ => .standard,
    .video => .video,
    .music => .music,
    .profile => .profile,
  };
}

extension UiLinkPreviewCardX on UiLinkPreviewCard {
  /// Card data for the DS link preview, with an image of the given size.
  LinkPreviewData toData({
    double? imageWidth,
    double? imageHeight,
    String? imageAlt,
  }) => LinkPreviewData(
    url: url,
    title: title,
    description: description.isEmpty ? null : description,
    imageWidth: imageWidth,
    imageHeight: imageHeight,
    imageAlt: imageAlt,
    kind: kind.toDs(),
  );
}

extension UiLinkPreviewX on UiLinkPreview {
  /// Card data for the DS link preview. The image counts only when it has
  /// image metadata.
  LinkPreviewData toData() {
    final metadata = image?.attachment.imageMetadata;
    return card.toData(
      imageWidth: metadata?.width.toDouble(),
      imageHeight: metadata?.height.toDouble(),
      imageAlt: image?.alt,
    );
  }
}
