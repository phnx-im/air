// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/attachments/attachment_image_provider.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_blurhash/flutter_blurhash.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// A small square still of an image attachment, for the places that refer to a
/// message rather than show it -- the quote above a reply, and that same quote
/// staged in the composer.
///
/// Falls back to the blurhash, and then to a plain fill, so a thumbnail this
/// size never becomes a reason to fetch an attachment the reader has not
/// opened.
class AttachmentThumbnail extends StatelessWidget {
  const AttachmentThumbnail({
    super.key,
    required this.attachment,
    required this.size,
    required this.radius,
  });

  final UiAttachment attachment;
  final double size;
  final double radius;

  @override
  Widget build(BuildContext context) {
    final metadata = attachment.imageMetadata;
    final pixels = (size * MediaQuery.devicePixelRatioOf(context)).round();

    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: SizedBox.square(
        dimension: size,
        child: Stack(
          fit: .expand,
          children: [
            ColoredBox(color: SemanticPalette.of(context).fill.tertiary),
            if (metadata != null) BlurHash(hash: metadata.blurhash),
            Image(
              image: ResizeImage(
                AttachmentImageProvider(
                  attachment: attachment,
                  attachmentsRepository: context.read<AttachmentsRepository>(),
                ),
                width: pixels,
                height: pixels,
                policy: .fit,
                allowUpscaling: false,
              ),
              fit: .cover,
              errorBuilder: (_, _, _) => const SizedBox.shrink(),
            ),
          ],
        ),
      ),
    );
  }
}
