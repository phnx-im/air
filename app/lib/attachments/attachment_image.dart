// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:air/attachments/attachment_image_provider.dart';
import 'package:air/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/ui/typography/font_size.dart';
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
/// - **Static**: rendered via [Image] + [AttachmentImageProvider], so frames
///   live in Flutter's shared `imageCache`. Tap forwards to [onTap] (image
///   viewer). The bytes loaded for classification are discarded; the provider
///   re-fetches them on first decode and the framework holds the result from
///   then on.
///
/// - **Animated**: a fresh codec is instantiated per mount and frames are
///   driven by a [Timer]. Autoplays up to [_maxAutoLoops] then freezes on the
///   last frame. Tapping toggles playback (running → freeze on current frame;
///   stopped → replay from the start). [onTap] is unused as animated
///   attachments intercept the gesture. Frames are not cached; each mount
///   drives its own per-widget animation state.
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
  final VoidCallback? onTap;

  @override
  State<AttachmentImage> createState() => _AttachmentImageState();
}

class _AttachmentImageState extends State<AttachmentImage> {
  /// Per-session memo of the animated-vs-static classification.
  static final Map<AttachmentId, bool> _animationFlagCache = {};

  Uint8List? _bytes;
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
    unawaited(_load());
  }

  /// Loads the encoded bytes and (if not already memoized) classifies them.
  Future<void> _load() async {
    final id = widget.attachment.attachmentId;
    try {
      if (!mounted) return;
      final loaded = await context
          .read<AttachmentsRepository>()
          .loadImageAttachment(attachmentId: id, chunkEventCallback: (_) {});
      if (!mounted) return;
      _animationFlagCache[id] = loaded.isAnimated;
      if (!loaded.isAnimated) {
        setState(() => _isAnimated = false);
        return;
      }
      _bytes = loaded.bytes;
      if (_isAnimated == null) {
        setState(() => _isAnimated = true);
      }
      await _instantiateAndPlay();
    } catch (e, st) {
      _log.severe('Failed to load attachment', e, st);
      if (mounted) setState(() => _error = e);
    }
  }

  /// Instantiates a fresh codec from the held bytes and renders the first
  /// frame, then schedules the rest of the animation.
  Future<void> _instantiateAndPlay({bool userInitiated = false}) async {
    final bytes = _bytes;
    if (bytes == null) return;
    final gen = ++_playGeneration;

    _frameTimer?.cancel();
    _frameTimer = null;
    _codec?.dispose();
    _codec = null;

    try {
      final buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
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
  void _onTap() {
    switch (_isAnimated) {
      case true:
        _toggleAnimation();
      case false:
        widget.onTap?.call();
      case null:
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
    final blurhash = BlurHash(hash: widget.imageMetadata.blurhash);

    final Widget? foreground;
    if (_error != null) {
      foreground = const Align(child: AppIcon.circleAlert(size: 32));
    } else if (_isAnimated == false) {
      foreground = Image(
        image: AttachmentImageProvider(
          attachment: widget.attachment,
          attachmentsRepository: context.read<AttachmentsRepository>(),
        ),
        fit: widget.fit,
        alignment: Alignment.center,
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
      fit: StackFit.expand,
      children: [blurhash, if (foreground != null) foreground],
    );

    return AspectRatio(
      aspectRatio: widget.imageMetadata.width / widget.imageMetadata.height,
      child: Stack(
        fit: StackFit.expand,
        children: [
          GestureDetector(onTap: _onTap, child: content),
          if (widget.isSender)
            AttachmentUploadStatus(
              attachmentId: widget.attachment.attachmentId,
              size: widget.attachment.size,
            ),
        ],
      ),
    );
  }
}

class AttachmentUploadStatus extends HookWidget {
  const AttachmentUploadStatus({
    super.key,
    required this.attachmentId,
    required this.size,
  });

  final AttachmentId attachmentId;
  final int size;

  @override
  Widget build(BuildContext context) {
    final uploadStatusSteam = useMemoized(
      () => context.read<AttachmentsRepository>().statusStream(
        attachmentId: attachmentId,
      ),
      [attachmentId],
    );
    final uploadStatus = useStream<UiAttachmentStatus>(uploadStatusSteam);

    final loc = AppLocalizations.of(context);

    return Align(
      alignment: Alignment.center,
      child: switch (uploadStatus.data) {
        null || UiAttachmentStatus_Completed() => const SizedBox.shrink(),
        UiAttachmentStatus_Pending() ||
        UiAttachmentStatus_Failed() => OutlinedButton(
          onPressed: () {
            context.read<ChatDetailsCubit>().retryUploadAttachment(
              attachmentId,
            );
          },
          child: Row(
            mainAxisAlignment: .center,
            mainAxisSize: MainAxisSize.min,
            children: [
              const AppIcon.upload(size: 16),
              const SizedBox(width: Spacings.xxxs),
              Text(
                loc.attachment_tryAgain,
                style: TextStyle(
                  color: CustomColorScheme.of(context).text.primary,
                  fontSize: LabelFontSize.base.size,
                ),
              ),
            ],
          ),
        ),
        UiAttachmentStatus_Progress(field0: final loaded) => ClipRRect(
          borderRadius: BorderRadius.circular(100),
          child: BackdropFilter(
            filter: ui.ImageFilter.blur(sigmaX: 10, sigmaY: 10),
            child: Padding(
              padding: const EdgeInsets.all(Spacings.xs),
              child: Stack(
                alignment: Alignment.center,
                children: [
                  CircularProgressIndicator(
                    strokeWidth: 2,
                    valueColor: AlwaysStoppedAnimation<Color>(
                      CustomColorScheme.of(context).text.primary,
                    ),
                    backgroundColor: Colors.transparent,
                    value: loaded / BigInt.from(size),
                  ),
                  IconButton(
                    onPressed: () {
                      context.read<AttachmentsRepository>().cancel(
                        attachmentId: attachmentId,
                      );
                    },
                    icon: const AppIcon.x(size: 24),
                  ),
                ],
              ),
            ),
          ),
        ),
      },
    );
  }
}
