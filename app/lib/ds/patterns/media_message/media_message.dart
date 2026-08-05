// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/media_message/media_message_tokens.dart';
import 'package:flutter/widgets.dart';

/// A picture in a message bubble, sized by the rules the design gives it:
///
///   * thumbnail -- both natural dimensions under
///     [MediaMessageTokens.thumbnailMin]: a fixed square bubble with the
///     picture inset inside it, so a sticker-sized image isn't blown up.
///   * skyscraper -- so tall that fitting it to [MediaMessageTokens.maxHeight]
///     would leave it narrower than [MediaMessageTokens.minScaleWidth]: cropped
///     to cover a frame of that width instead.
///   * default -- the bubble takes the picture's own aspect ratio, scaled down
///     to the available width and the height cap, never up, so it hugs the
///     picture rather than letterboxing it.
///
/// Without natural dimensions the bubble hugs whatever the picture decodes to,
/// under the same height cap.
///
/// A pure view: the pixels, the stand-in shown while they load, and any status
/// affordance on top all arrive as slots, so the host owns the transfer and the
/// pattern owns the geometry.
class MediaMessage extends StatelessWidget {
  const MediaMessage({
    super.key,
    required ImageProvider this.image,
    this.naturalWidth,
    this.naturalHeight,
    this.isSelf = false,
    this.placeholder,
    this.error,
    this.overlay,
    this.onTap,
  }) : builder = null;

  /// Pixels the host decodes itself, an animation it steps frame by frame being
  /// the case that needs it. [builder] receives the [BoxFit] the chosen branch
  /// needs, so host-driven frames land in exactly the same frame a provider
  /// would.
  const MediaMessage.builder({
    super.key,
    required Widget Function(BoxFit fit) this.builder,
    this.naturalWidth,
    this.naturalHeight,
    this.isSelf = false,
    this.placeholder,
    this.overlay,
    this.onTap,
  }) : image = null,
       error = null;

  /// The picture. Null exactly when [builder] is set.
  final ImageProvider? image;

  /// Renders the picture from pixels the host holds. Null exactly when [image]
  /// is set.
  final Widget Function(BoxFit fit)? builder;

  /// The picture's own pixel size, where the host knows it ahead of the decode.
  /// Both or neither: one alone leaves the aspect ratio unknown, which is what
  /// the rules turn on.
  final double? naturalWidth;
  final double? naturalHeight;

  /// Whether the bubble is the local user's. Picks the bubble fill.
  final bool isSelf;

  /// Painted behind the picture, inside the bubble: a blurhash or another cheap
  /// stand-in that holds the frame while the full picture decodes.
  final Widget? placeholder;

  /// Replaces the picture when it fails to decode. Defaults to a broken-image
  /// glyph on a flat fill.
  final Widget? error;

  /// Centered on the bubble, over the picture: transfer progress, a retry
  /// affordance, a paused-animation badge. It rides on the frame because only
  /// the frame knows the size the rules settled on.
  final Widget? overlay;

  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    Widget content = _MediaFrame(
      fill: isSelf
          ? palette.message.selfBackground
          : palette.message.otherBackground,
      naturalWidth: naturalWidth,
      naturalHeight: naturalHeight,
      buildImage: _buildImage,
    );

    if (overlay != null) {
      content = Stack(
        alignment: Alignment.center,
        children: [content, overlay!],
      );
    }

    return onTap == null
        ? content
        : GestureDetector(onTap: onTap, child: content);
  }

  Widget _buildImage(BoxFit fit) {
    final picture =
        builder?.call(fit) ??
        _ProviderImage(
          image: image!,
          fit: fit,
          error: error ?? const _MediaError(),
        );

    if (placeholder == null) return picture;
    // The picture sizes the stack and the stand-in stretches to it, so this
    // holds in the branch where the frame has no size of its own either.
    return Stack(
      children: [
        Positioned.fill(child: placeholder!),
        picture,
      ],
    );
  }
}

/// The bubble around the picture, and the rules that size it. Split out so both
/// [MediaMessage] constructors reach the same geometry.
class _MediaFrame extends StatelessWidget {
  const _MediaFrame({
    required this.fill,
    required this.naturalWidth,
    required this.naturalHeight,
    required this.buildImage,
  });

  final Color fill;
  final double? naturalWidth;
  final double? naturalHeight;
  final Widget Function(BoxFit fit) buildImage;

  @override
  Widget build(BuildContext context) {
    final width = naturalWidth;
    final height = naturalHeight;

    if (width == null || height == null) {
      return _bubble(
        child: ConstrainedBox(
          constraints: const BoxConstraints(
            maxHeight: MediaMessageTokens.maxHeight,
          ),
          child: buildImage(BoxFit.contain),
        ),
      );
    }

    if (width < MediaMessageTokens.thumbnailMin &&
        height < MediaMessageTokens.thumbnailMin) {
      return _thumbnail();
    }

    if (width / height * MediaMessageTokens.maxHeight <
        MediaMessageTokens.minScaleWidth) {
      return _bubble(
        width: MediaMessageTokens.minScaleWidth,
        height: MediaMessageTokens.maxHeight,
        child: buildImage(BoxFit.cover),
      );
    }

    return LayoutBuilder(
      builder: (context, constraints) {
        final available = constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : width;
        final scale = math.min(
          1.0,
          math.min(available / width, MediaMessageTokens.maxHeight / height),
        );
        return _bubble(
          width: width * scale,
          height: height * scale,
          child: buildImage(BoxFit.cover),
        );
      },
    );
  }

  Widget _bubble({double? width, double? height, required Widget child}) =>
      Container(
        width: width,
        height: height,
        decoration: BoxDecoration(
          color: fill,
          borderRadius: BorderRadius.circular(MediaMessageTokens.radius),
        ),
        clipBehavior: Clip.antiAlias,
        child: child,
      );

  Widget _thumbnail() {
    // Concentric with the bubble: the inner radius drops by the inset, so the
    // two curves stay parallel instead of the picture's corners fighting the
    // frame's.
    final inner =
        (MediaMessageTokens.radius - MediaMessageTokens.thumbnailPadding).clamp(
          0.0,
          MediaMessageTokens.radius,
        );
    return Container(
      width: MediaMessageTokens.thumbnailMin,
      height: MediaMessageTokens.thumbnailMin,
      padding: const EdgeInsets.all(MediaMessageTokens.thumbnailPadding),
      decoration: BoxDecoration(
        color: fill,
        borderRadius: BorderRadius.circular(MediaMessageTokens.radius),
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(inner),
        child: buildImage(BoxFit.cover),
      ),
    );
  }
}

/// The picture, decoded no larger than the box it paints into.
class _ProviderImage extends StatelessWidget {
  const _ProviderImage({
    required this.image,
    required this.fit,
    required this.error,
  });

  final ImageProvider image;
  final BoxFit fit;
  final Widget error;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final dpr = MediaQuery.devicePixelRatioOf(context);
        final width = constraints.maxWidth.isFinite
            ? (constraints.maxWidth * dpr).round()
            : null;
        final height = constraints.maxHeight.isFinite
            ? (constraints.maxHeight * dpr).round()
            : null;
        return Image(
          // The box comes from the sender-declared size, which may not match
          // the actual pixels. `exact` would decode to the box like
          // BoxFit.fill and distort the picture, so constrain the decode
          // instead of reshaping it.
          image: width == null && height == null
              ? image
              : ResizeImage(
                  image,
                  width: width,
                  height: height,
                  policy: ResizeImagePolicy.fit,
                  allowUpscaling: false,
                ),
          fit: fit,
          alignment: Alignment.center,
          errorBuilder: (context, error, stackTrace) => this.error,
        );
      },
    );
  }
}

/// Stand-in for a picture that won't decode. Square at the thumbnail size
/// where the frame has no size of its own, stretched to the frame where it has.
class _MediaError extends StatelessWidget {
  const _MediaError();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return SizedBox.square(
      dimension: MediaMessageTokens.thumbnailMin,
      child: ColoredBox(
        color: palette.fill.tertiary,
        child: Center(
          child: AppIcon(
            type: AppIconType.imageOff,
            size: MediaMessageTokens.errorIconSize,
            color: palette.text.quaternary,
          ),
        ),
      ),
    );
  }
}
