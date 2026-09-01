// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:ui' as ui;

import 'package:air/features/attachments/attachment_thumbnail_provider.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_blurhash/flutter_blurhash.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';

final _log = Logger('AttachmentImage');

/// Maximum number of times an animated attachment plays automatically.
const int _maxAutoLoops = 3;

/// Renders an attachment image loaded from the database.
///
/// Depending on whether the image is animated, it is rendered differently::
///
/// - **Static**: rendered via [Image] + [AttachmentThumbnailProvider], so frames
///   live in Flutter's shared `imageCache`. Tap forwards to [onTap] (image
///   viewer) along with the provider painted here, so the viewer opens on a
///   decode it already has. Classification only asks for the animated flag;
///   the provider is the sole reader of the thumbnail bytes and the framework
///   holds the decode from then on.
///
/// - **Animated**: the original bytes are read from the database and a fresh
///   codec is instantiated per mount (and per replay); frames are driven by a
///   [Timer]. Autoplays up to [_maxAutoLoops] then freezes on the last frame.
///   Tapping toggles playback (running → freeze on current frame; stopped →
///   replay from the start). [onTap] is unused as animated attachments
///   intercept the gesture. Neither bytes nor frames are held in widget state
///   beyond the current frame; each mount drives its own animation.
class AttachmentImage extends StatefulWidget {
  const AttachmentImage({
    super.key,
    required this.attachment,
    required this.imageMetadata,
    required this.fit,
    required this.isSender,
    this.onTap,
  });

  final UiAttachment attachment;
  final UiImageMetadata imageMetadata;
  final BoxFit fit;
  final bool isSender;

  /// Receives the provider the still picture is painted from. Unused on the
  /// animated branch, which keeps the tap for its own playback.
  final void Function(ImageProvider thumbnail)? onTap;

  @override
  State<AttachmentImage> createState() => _AttachmentImageState();
}

class _AttachmentImageState extends State<AttachmentImage> {
  /// Per-session memo of the animated-vs-static classification.
  static final Map<AttachmentId, bool> _animationFlagCache = {};

  ui.Codec? _codec;
  ui.Image? _currentFrame;
  Timer? _frameTimer;
  int _nextFrameIndex = 0;
  int _completedLoops = 0;
  bool _stopped = false;
  bool? _isAnimated;
  Object? _error;
  bool _initialized = false;

  /// Generation counter for [_instantiateAndPlay].
  int _playGeneration = 0;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_initialized) return;
    _initialized = true;
    final cached = _animationFlagCache[widget.attachment.attachmentId];
    if (cached == false) {
      // We let [Image] handle the static image
      _isAnimated = false;
      return;
    }
    if (cached == true) {
      _isAnimated = true;
    }
    unawaited(_classify());
  }

  /// Classifies the attachment as animated or static, downloading it and
  /// generating its thumbnail if needed, and starts playback when animated.
  Future<void> _classify({bool retryDownloadIfFailed = false}) async {
    final id = widget.attachment.attachmentId;
    try {
      if (!mounted) return;
      final repository = context.read<AttachmentsRepository>();
      final isAnimated = await repository.isAttachmentAnimated(
        attachmentId: id,
        retryDownloadIfFailed: retryDownloadIfFailed,
      );
      if (!mounted || isAnimated == null) return;
      _animationFlagCache[id] = isAnimated;
      setState(() => _isAnimated = isAnimated);
      if (isAnimated) {
        await _instantiateAndPlay();
      }
    } catch (e, st) {
      _log.severe('Failed to classify attachment', e, st);
      if (mounted) setState(() => _error = e);
    }
  }

  /// Instantiates a fresh codec from the loaded bytes and renders the first
  /// frame, then schedules the rest of the animation.
  Future<void> _instantiateAndPlay({bool userInitiated = false}) async {
    if (!mounted) return;
    final repository = context.read<AttachmentsRepository>();
    final gen = ++_playGeneration;

    _frameTimer?.cancel();
    _frameTimer = null;
    _codec?.dispose();
    _codec = null;

    try {
      final original = await repository.loadImageAttachment(
        attachmentId: widget.attachment.attachmentId,
        retryDownloadIfFailed: false,
      );
      if (gen != _playGeneration || !mounted) return;

      final buffer = await ui.ImmutableBuffer.fromUint8List(original.bytes);
      if (gen != _playGeneration || !mounted) {
        buffer.dispose();
        return;
      }
      final codec = await ui.instantiateImageCodecFromBuffer(buffer);
      if (gen != _playGeneration || !mounted) {
        codec.dispose();
        return;
      }
      _completedLoops = 0;
      _nextFrameIndex = 0;

      final first = await codec.getNextFrame();
      if (gen != _playGeneration || !mounted) {
        first.image.dispose();
        codec.dispose();
        return;
      }

      _codec = codec;
      final old = _currentFrame;
      setState(() {
        _currentFrame = first.image;
        _stopped = false;
      });
      old?.dispose();
      _nextFrameIndex = (_nextFrameIndex + 1) % codec.frameCount;

      final disableAnimations = MediaQuery.of(context).disableAnimations;
      if (disableAnimations && !userInitiated) {
        _stopped = true;
        return;
      }
      _frameTimer = Timer(first.duration, _showNextFrame);
    } catch (e, st) {
      _log.severe('Failed to play attachment animation', e, st);
      if (mounted && gen == _playGeneration) {
        setState(() => _error = e);
      }
    }
  }

  /// Renders the next codec frame and schedules the one after it.
  Future<void> _showNextFrame() async {
    final codec = _codec;
    if (codec == null || _stopped) return;
    final frame = await codec.getNextFrame();
    if (!mounted || _stopped || _codec != codec) {
      frame.image.dispose();
      return;
    }
    final old = _currentFrame;
    setState(() => _currentFrame = frame.image);
    old?.dispose();

    _nextFrameIndex = (_nextFrameIndex + 1) % codec.frameCount;
    if (_nextFrameIndex == 0) {
      _completedLoops++;
      if (_completedLoops >= _maxAutoLoops) {
        _stopped = true;
        return;
      }
    }
    _frameTimer = Timer(frame.duration, _showNextFrame);
  }

  /// Routes the gesture: animated attachments toggle their own playback,
  /// static attachments forward to the caller-provided [onTap].
  void _onTap(ImageProvider? thumbnail) {
    switch (_isAnimated) {
      case true:
        _toggleAnimation();
      case false when thumbnail != null:
        widget.onTap?.call(thumbnail);
      case _:
        break;
    }
  }

  /// Stops a running animation, or restarts a stopped one from the first frame.
  void _toggleAnimation() {
    if (_stopped) {
      unawaited(_instantiateAndPlay(userInitiated: true));
    } else {
      _frameTimer?.cancel();
      _frameTimer = null;
      _codec?.dispose();
      _codec = null;
      setState(() => _stopped = true);
    }
  }

  @override
  void dispose() {
    _frameTimer?.cancel();
    _codec?.dispose();
    _currentFrame?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AspectRatio(
      aspectRatio: widget.imageMetadata.width / widget.imageMetadata.height,
      // The still picture's decode is bounded by the box we lay out in, so the
      // provider only exists once that box is known -- and the tap has to hand
      // that same provider on.
      child: LayoutBuilder(builder: _content),
    );
  }

  Widget _content(BuildContext context, BoxConstraints constraints) {
    // Non-null exactly on the static branch: an animated attachment steps its
    // own frames rather than going through a provider.
    final thumbnail = _isAnimated == false
        ? _thumbnail(context, constraints)
        : null;

    final Widget? foreground;
    if (thumbnail != null) {
      foreground = Image(
        image: thumbnail,
        fit: widget.fit,
        alignment: Alignment.center,
        errorBuilder: (_, _, _) => const SizedBox.shrink(),
      );
    } else if (_currentFrame != null) {
      foreground = RawImage(
        image: _currentFrame,
        fit: widget.fit,
        alignment: Alignment.center,
      );
    } else {
      foreground = null;
    }

    final content = Stack(
      fit: .expand,
      children: [
        BlurHash(hash: widget.imageMetadata.blurhash),
        ?foreground,
      ],
    );

    final isAnimationPaused = _isAnimated == true && _stopped && _error == null;

    return Stack(
      fit: .expand,
      children: [
        GestureDetector(onTap: () => _onTap(thumbnail), child: content),
        AttachmentImageOverlay(
          attachmentId: widget.attachment.attachmentId,
          size: widget.attachment.size,
          isSender: widget.isSender,
          isAnimationPaused: isAnimationPaused,
          onTapDownload: () => _classify(retryDownloadIfFailed: true),
        ),
      ],
    );
  }

  /// The provider the still picture is painted from: a decode bounded by the
  /// box on screen, so the list holds thumbnails rather than full frames.
  ImageProvider _thumbnail(BuildContext context, BoxConstraints constraints) {
    final dpr = MediaQuery.devicePixelRatioOf(context);
    return ResizeImage(
      AttachmentThumbnailProvider(
        attachmentId: widget.attachment.attachmentId,
        attachmentsRepository: context.read<AttachmentsRepository>(),
      ),
      // The box we lay out in comes from the sender-declared metadata, which
      // may not match the actual pixels. `exact` (the default) would decode to
      // the box like .fill and distort the image, so constrain the decode
      // instead of reshaping it.
      policy: .fit,
      width: constraints.maxWidth.isFinite
          ? (constraints.maxWidth * dpr).round()
          : null,
      height: constraints.maxHeight.isFinite
          ? (constraints.maxHeight * dpr).round()
          : null,
      allowUpscaling: false,
    );
  }
}

class AttachmentImageOverlay extends HookWidget {
  const AttachmentImageOverlay({
    super.key,
    required this.attachmentId,
    required this.size,
    required this.isSender,
    required this.isAnimationPaused,
    required this.onTapDownload,
  });

  final AttachmentId attachmentId;
  final int size;
  final bool isSender;
  final bool isAnimationPaused;

  final VoidCallback onTapDownload;

  @override
  Widget build(BuildContext context) {
    final retries = useState(0); // bump to force stream re-subscription
    final statusStream = useMemoized(
      () => context.read<AttachmentsRepository>().statusStream(
        attachmentId: attachmentId,
      ),
      [attachmentId, retries.value],
    );
    final status = useStream<UiAttachmentStatus>(statusStream);

    final palette = SemanticPalette.of(context);

    return Align(
      alignment: Alignment.center,
      child: switch (status.data) {
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() when isSender => _BlurredPill(
          child: ButtonIcon(
            variant: ButtonIconVariant.plain,
            icon: AppIconType.upload,
            iconSize: S.s24,
            hitTargetSize: S.s48,
            onPressed: () {
              retries.value++;
              context.read<ChatDetailsCubit>().retryUploadAttachment(
                attachmentId,
              );
            },
          ),
        ),
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() => _BlurredPill(
          child: ButtonIcon(
            variant: ButtonIconVariant.plain,
            icon: AppIconType.download,
            iconSize: S.s24,
            hitTargetSize: S.s48,
            onPressed: () {
              retries.value++;
              onTapDownload();
            },
          ),
        ),
        UiAttachmentStatus_Progress(field0: final loaded) => _BlurredPill(
          child: Stack(
            alignment: Alignment.center,
            children: [
              CircularProgressIndicator(
                strokeWidth: StrokeWidth.px2,
                valueColor: AlwaysStoppedAnimation<Color>(palette.text.primary),
                backgroundColor: Colors.transparent,
                value: loaded / BigInt.from(size),
              ),
              ButtonIcon(
                variant: ButtonIconVariant.plain,
                icon: AppIconType.x,
                iconSize: S.s24,
                hitTargetSize: S.s48,
                onPressed: () {
                  context.read<AttachmentsRepository>().cancel(
                    attachmentId: attachmentId,
                  );
                },
              ),
            ],
          ),
        ),
        UiAttachmentStatus_NotFound() => ButtonIcon(
          variant: ButtonIconVariant.plain,
          icon: AppIconType.circleAlert,
          size: ButtonIconSize.s48,
          iconSize: S.s32,
          iconColor: palette.text.primary,
          onPressed: () {
            showSnackBarStandalone(
              (loc) => SnackBar(content: Text(loc.attachment_notFound)),
              tone: SnackbarTone.danger,
            );
          },
        ),
        UiAttachmentStatus_Completed() when isAnimationPaused => IgnorePointer(
          child: _BlurredPill(
            child: AppIcon.rotateCw(size: 24, color: palette.text.primary),
          ),
        ),
        null || UiAttachmentStatus_Completed() => const SizedBox.shrink(),
      },
    );
  }
}

/// Centered blurred pill used as the visual container for image overlays.
class _BlurredPill extends StatelessWidget {
  const _BlurredPill({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(CornerRadius.full),
      child: BackdropFilter(
        filter: ui.ImageFilter.blur(
          sigmaX: Effect.blur(BlurLevel.medium),
          sigmaY: Effect.blur(BlurLevel.medium),
        ),
        child: ColoredBox(
          color: SemanticPalette.of(context).backgroundMaterial.tertiary,
          child: Padding(padding: const EdgeInsets.all(S.s16), child: child),
        ),
      ),
    );
  }
}
