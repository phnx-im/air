// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/link_preview/link_preview_tokens.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:flutter/widgets.dart';

/// What a link points at, from the page's `og:type`.
///
/// Only the kinds that change how the card reads are modeled, everything else
/// is [standard].
enum LinkPreviewKind { standard, video, music, profile }

/// Everything a link card renders, read from a page's Open Graph markup.
///
/// The sender resolves it once and it travels inside the message, so every
/// field is sender controlled. That is why the card's last line is [url].
@immutable
class LinkPreviewData {
  const LinkPreviewData({
    required this.url,
    required this.title,
    this.description,
    this.imageWidth,
    this.imageHeight,
    this.imageAlt,
    this.kind = LinkPreviewKind.standard,
  });

  /// The address as typed in the message, never `og:url`.
  final String url;

  /// `og:title`, falling back to the page's `<title>`.
  final String title;

  /// `og:description`, falling back to the meta description.
  final String? description;

  /// Natural image size. Both null means no image, and the card is text only.
  final double? imageWidth;
  final double? imageHeight;

  /// `og:image:alt`, the image's semantics label.
  final String? imageAlt;

  final LinkPreviewKind kind;
}

enum _LinkShape { banner, thumbnail, textOnly }

_LinkShape _shapeFor(LinkPreviewData d, {required bool hasImage}) {
  final w = d.imageWidth;
  final h = d.imageHeight;
  if (!hasImage || w == null || h == null) return _LinkShape.textOnly;
  // A profile picture is a person, so it reads as a thumbnail.
  if (d.kind == LinkPreviewKind.profile) return _LinkShape.thumbnail;
  // Bogus dimensions get the banner: a wrong guess costs only a crop.
  if (w <= 0 || h <= 0) return _LinkShape.banner;
  if (w < LinkPreviewTokens.bannerMinWidth) return _LinkShape.thumbnail;
  final aspect = w / h;
  if (aspect < LinkPreviewTokens.bannerMinAspect ||
      aspect > LinkPreviewTokens.bannerMaxAspect) {
    return _LinkShape.thumbnail;
  }
  return _LinkShape.banner;
}

/// Whether [data] renders with a full-bleed banner image.
///
/// A host nesting the preview uses this to decide whether its own top inset
/// applies. Assumes the host passes an image builder whenever [data] has
/// image dimensions.
bool linkPreviewLeadsWithBanner(LinkPreviewData data) =>
    _shapeFor(data, hasImage: true) == _LinkShape.banner;

/// Link preview card.
///
/// While [data] is null the preview is being fetched and the card shows [url]
/// next to a filling ring. With [data] it takes one of three shapes: a
/// full-bleed banner above the text, a square thumbnail beside it, or text
/// only. The text always reads title, description, source.
class LinkPreview extends StatelessWidget {
  const LinkPreview({
    super.key,
    required this.url,
    this.data,
    this.imageBuilder,
    this.isSelf = false,
    this.nested = false,
    this.nestedPadding = EdgeInsets.zero,
    this.onTap,
  });

  /// The address, shown on the pending card.
  final String url;

  /// The resolved metadata. Null while the preview is being fetched.
  final LinkPreviewData? data;

  /// Paints the preview image, cropped with the given fit. Without it the card
  /// is text only, whatever the image dimensions say.
  final Widget Function(BoxFit fit)? imageBuilder;

  /// Own message. Picks the bubble fill.
  final bool isSelf;

  /// Renders without fill, corners or clip, for a host bubble that provides
  /// them. A banner runs to the bubble's edges and the text takes
  /// [nestedPadding].
  final bool nested;

  /// Inset of the text block when [nested]. The banner ignores it.
  final EdgeInsets nestedPadding;

  /// Fired when the card is tapped. The whole card is the target.
  final VoidCallback? onTap;

  /// Color of the hairline between a nested card and the message text.
  static Color dividerColorOf(BuildContext context) =>
      SemanticPalette.of(context).separator.secondary;

  @override
  Widget build(BuildContext context) {
    final d = data;
    final fill = MessageBubble.fillOf(context, isSelf: isSelf);

    final Widget card;
    if (d == null) {
      card = _LinkPendingCard(url: url, fill: fill);
    } else {
      final image = imageBuilder;
      final shape = _shapeFor(d, hasImage: image != null);
      card = switch ((shape, nested)) {
        (_LinkShape.banner, false) => ClipRRect(
          borderRadius: BorderRadius.circular(LinkPreviewTokens.radius),
          child: ColoredBox(
            color: fill,
            child: _bannerColumn(d, image!, LinkPreviewTokens.padding),
          ),
        ),
        (_LinkShape.banner, true) => _bannerColumn(d, image!, nestedPadding),
        (_LinkShape.thumbnail, false) => _CardBox(
          fill: fill,
          child: _LinkThumbnailRow(data: d, imageBuilder: image!),
        ),
        (_LinkShape.thumbnail, true) => Padding(
          padding: nestedPadding,
          child: _LinkThumbnailRow(data: d, imageBuilder: image!),
        ),
        (_LinkShape.textOnly, false) => _CardBox(
          fill: fill,
          child: _LinkTextBlock(data: d),
        ),
        (_LinkShape.textOnly, true) => Padding(
          padding: nestedPadding,
          child: _LinkTextBlock(data: d),
        ),
      };
    }

    // One node for the whole card, read as title, description, address.
    if (onTap == null) return MergeSemantics(child: card);
    return MergeSemantics(
      child: Semantics(
        link: true,
        child: MouseRegion(
          cursor: SystemMouseCursors.click,
          child: GestureDetector(behavior: .opaque, onTap: onTap, child: card),
        ),
      ),
    );
  }

  Widget _bannerColumn(
    LinkPreviewData d,
    Widget Function(BoxFit fit) imageBuilder,
    EdgeInsets textPadding,
  ) => Column(
    mainAxisSize: .min,
    crossAxisAlignment: .stretch,
    children: [
      _LinkBannerImage(data: d, imageBuilder: imageBuilder),
      Padding(
        padding: textPadding,
        child: _LinkTextBlock(data: d),
      ),
    ],
  );
}

/// Filled, rounded card around the thumbnail and text-only shapes.
class _CardBox extends StatelessWidget {
  const _CardBox({required this.fill, required this.child});

  final Color fill;
  final Widget child;

  @override
  Widget build(BuildContext context) => Container(
    padding: LinkPreviewTokens.padding,
    decoration: BoxDecoration(
      color: fill,
      borderRadius: BorderRadius.circular(LinkPreviewTokens.radius),
    ),
    child: child,
  );
}

/// The card while the preview is fetched: the address next to a filling ring.
///
/// It reflows into the final shape once the metadata arrives, instead of
/// reserving space for a banner that may never come.
class _LinkPendingCard extends StatelessWidget {
  const _LinkPendingCard({required this.url, required this.fill});

  final String url;
  final Color fill;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final dark = palette.brightness == .dark;
    final accent = dark ? palette.function.link : palette.text.secondary;
    return Container(
      padding: LinkPreviewTokens.pendingPadding,
      decoration: BoxDecoration(
        color: fill,
        borderRadius: BorderRadius.circular(LinkPreviewTokens.radius),
      ),
      child: Row(
        mainAxisSize: .min,
        children: [
          ExcludeSemantics(
            child: TweenAnimationBuilder<double>(
              tween: Tween(begin: 0, end: 1),
              duration: LinkPreviewTokens.pendingDuration,
              curve: Curves.easeInOut,
              builder: (_, value, _) => _PendingRing(
                value: value,
                trackColor: palette.text.quaternary,
                // Same color as the address, so the fill stands out from the
                // track in both modes.
                progressColor: accent,
              ),
            ),
          ),
          const SizedBox(width: LinkPreviewTokens.pendingGap),
          Flexible(
            child: Text(
              url,
              maxLines: 1,
              overflow: .ellipsis,
              style: typeScale.body.s.style(
                color: accent,
                weight: Weight.emphasized,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Determinate ring, filled clockwise from the top up to [value].
class _PendingRing extends StatelessWidget {
  const _PendingRing({
    required this.value,
    required this.trackColor,
    required this.progressColor,
  });

  final double value;
  final Color trackColor;
  final Color progressColor;

  @override
  Widget build(BuildContext context) => SizedBox.square(
    dimension: LinkPreviewTokens.pendingRingSize,
    child: CustomPaint(
      painter: _RingPainter(
        value: value,
        trackColor: trackColor,
        progressColor: progressColor,
      ),
    ),
  );
}

class _RingPainter extends CustomPainter {
  const _RingPainter({
    required this.value,
    required this.trackColor,
    required this.progressColor,
  });

  final double value;
  final Color trackColor;
  final Color progressColor;

  @override
  void paint(Canvas canvas, Size size) {
    const stroke = LinkPreviewTokens.pendingRingStroke;
    final rect = (Offset.zero & size).deflate(stroke / 2);
    final paint = Paint()
      ..style = .stroke
      ..strokeWidth = stroke;
    canvas.drawArc(rect, 0, 2 * math.pi, false, paint..color = trackColor);
    if (value <= 0) return;
    canvas.drawArc(
      rect,
      -math.pi / 2,
      2 * math.pi * value.clamp(0, 1),
      false,
      paint
        ..color = progressColor
        ..strokeCap = .round,
    );
  }

  @override
  bool shouldRepaint(_RingPainter old) =>
      old.value != value ||
      old.trackColor != trackColor ||
      old.progressColor != progressColor;
}

/// Banner image: 1.91:1 up to the height ceiling, then a wider crop.
class _LinkBannerImage extends StatelessWidget {
  const _LinkBannerImage({required this.data, required this.imageBuilder});

  final LinkPreviewData data;
  final Widget Function(BoxFit fit) imageBuilder;

  @override
  Widget build(BuildContext context) => ConstrainedBox(
    constraints: const BoxConstraints(
      maxHeight: LinkPreviewTokens.bannerMaxHeight,
    ),
    child: AspectRatio(
      aspectRatio: LinkPreviewTokens.bannerAspect,
      child: ClipRect(
        child: _LinkImage(data: data, imageBuilder: imageBuilder),
      ),
    ),
  );
}

/// Text block with a square thumbnail trailing it. A profile clips round, like
/// an avatar.
class _LinkThumbnailRow extends StatelessWidget {
  const _LinkThumbnailRow({required this.data, required this.imageBuilder});

  final LinkPreviewData data;
  final Widget Function(BoxFit fit) imageBuilder;

  @override
  Widget build(BuildContext context) {
    const size = LinkPreviewTokens.thumbSize;
    final image = SizedBox.square(
      dimension: size,
      child: _LinkImage(
        data: data,
        imageBuilder: imageBuilder,
        compactBadge: true,
      ),
    );
    return Row(
      mainAxisSize: .min,
      crossAxisAlignment: .start,
      children: [
        Expanded(child: _LinkTextBlock(data: data)),
        const SizedBox(width: LinkPreviewTokens.thumbGap),
        if (data.kind == LinkPreviewKind.profile)
          ClipOval(child: image)
        else
          ClipRRect(
            borderRadius: BorderRadius.circular(LinkPreviewTokens.thumbRadius),
            child: image,
          ),
      ],
    );
  }
}

/// The preview image, cropped to fill its box.
class _LinkImage extends StatelessWidget {
  const _LinkImage({
    required this.data,
    required this.imageBuilder,
    this.compactBadge = false,
  });

  final LinkPreviewData data;
  final Widget Function(BoxFit fit) imageBuilder;

  /// Smaller play badge, for the thumbnail.
  final bool compactBadge;

  @override
  Widget build(BuildContext context) {
    final picture = SizedBox.expand(child: imageBuilder(BoxFit.cover));
    final alt = data.imageAlt;
    // Without alt text the image is decorative, the card text says it all.
    final image = alt == null || alt.isEmpty
        ? ExcludeSemantics(child: picture)
        : Semantics(image: true, label: alt, child: picture);
    final playable =
        data.kind == LinkPreviewKind.video ||
        data.kind == LinkPreviewKind.music;
    if (!playable) return image;
    return Stack(
      fit: .expand,
      children: [
        image,
        Center(child: _PlayBadge(compact: compactBadge)),
      ],
    );
  }
}

/// Frosted circle with a play glyph, over a playable link's image.
class _PlayBadge extends StatelessWidget {
  const _PlayBadge({required this.compact});

  final bool compact;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return IgnorePointer(
      child: ExcludeSemantics(
        child: ClipOval(
          child: BackdropFilter(
            filter: ImageFilter.blur(
              sigmaX: LinkPreviewTokens.badgeBlur,
              sigmaY: LinkPreviewTokens.badgeBlur,
            ),
            child: ColoredBox(
              color: palette.backgroundMaterial.tertiary,
              child: Padding(
                padding: EdgeInsets.all(
                  compact
                      ? LinkPreviewTokens.badgePaddingCompact
                      : LinkPreviewTokens.badgePadding,
                ),
                child: AppIcon.play(
                  size: compact
                      ? LinkPreviewTokens.badgeIconSizeCompact
                      : LinkPreviewTokens.badgeIconSize,
                  color: palette.text.primary,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Title, optional description, then the source line, the same in every
/// shape.
class _LinkTextBlock extends StatelessWidget {
  const _LinkTextBlock({required this.data});

  final LinkPreviewData data;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final description = data.description;
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .start,
      children: [
        Text(
          data.title,
          maxLines: LinkPreviewTokens.titleMaxLines,
          overflow: .ellipsis,
          style: typeScale.body.s.style(
            color: palette.text.primary,
            weight: Weight.emphasized,
          ),
        ),
        if (description != null && description.isNotEmpty) ...[
          const SizedBox(height: LinkPreviewTokens.textGap),
          Text(
            description,
            maxLines: LinkPreviewTokens.descriptionMaxLines,
            overflow: .ellipsis,
            style: typeScale.body.xs.style(color: palette.text.tertiary),
          ),
        ],
        const SizedBox(height: LinkPreviewTokens.sourceGap),
        Row(
          mainAxisSize: .min,
          children: [
            AppIcon.globe(
              size: LinkPreviewTokens.iconSize,
              color: palette.text.quaternary,
            ),
            const SizedBox(width: LinkPreviewTokens.iconGap),
            Flexible(
              child: Text(
                data.url,
                maxLines: 1,
                overflow: .ellipsis,
                style: typeScale.body.xs.style(color: palette.text.quaternary),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
