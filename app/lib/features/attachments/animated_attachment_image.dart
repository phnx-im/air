// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:ui' as ui;

import 'package:air/core/core.dart';
import 'package:air/features/attachments/attachment_image_overlay.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';

final _log = Logger('AnimatedAttachmentImage');

/// Maximum number of times an animated attachment plays automatically.
const int _maxAutoLoops = 3;

/// Plays an animated image attachment from its original bytes.
///
/// The original bytes are read from the database and a fresh codec is
/// instantiated per mount (and per replay); frames are driven by a [Timer].
/// Autoplays on mount up to [_maxAutoLoops] then freezes on the last frame.
/// Tapping toggles playback (running → freeze on current frame; stopped →
/// replay from the start). Neither bytes nor frames are held in widget state
/// beyond the current frame; each mount drives its own animation.
///
/// Renders nothing until the first frame is decoded, so the caller's
/// placeholder shows through.
class AnimatedAttachmentImage extends StatefulWidget {
  const AnimatedAttachmentImage({
    super.key,
    required this.attachment,
    required this.fit,
    required this.isSender,
  });

  final UiAttachment attachment;
  final BoxFit fit;
  final bool isSender;

  @override
  State<AnimatedAttachmentImage> createState() =>
      _AnimatedAttachmentImageState();
}

class _AnimatedAttachmentImageState extends State<AnimatedAttachmentImage> {
  ui.Codec? _codec;
  ui.Image? _currentFrame;
  Timer? _frameTimer;
  int _nextFrameIndex = 0;
  int _completedLoops = 0;
  bool _stopped = false;
  Object? _error;
  bool _initialized = false;

  /// Generation counter for [_instantiateAndPlay].
  int _playGeneration = 0;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_initialized) return;
    _initialized = true;
    unawaited(_instantiateAndPlay());
  }

  @override
  void dispose() {
    _frameTimer?.cancel();
    _codec?.dispose();
    _currentFrame?.dispose();
    super.dispose();
  }

  /// Loads the original bytes, instantiates a fresh codec and renders the
  /// first frame, then schedules the rest of the animation.
  Future<void> _instantiateAndPlay({bool userInitiated = false}) async {
    if (!mounted) return;
    final repository = context.read<AttachmentsRepository>();
    final gen = ++_playGeneration;

    _frameTimer?.cancel();
    _frameTimer = null;
    _codec?.dispose();
    _codec = null;

    try {
      // Classification only reports animated once the attachment is on
      // device, so this is a plain database read.
      final original = await repository.loadImageAttachment(
        attachmentId: widget.attachment.attachmentId,
        retryDownloadIfFailed: false,
      );
      if (gen != _playGeneration || !mounted) return;

      final buffer = await ui.ImmutableBuffer.fromUint8List(original);
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
        setState(() => _stopped = true);
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
        setState(() => _stopped = true);
        return;
      }
    }
    _frameTimer = Timer(frame.duration, _showNextFrame);
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
  Widget build(BuildContext context) {
    return Stack(
      fit: .expand,
      children: [
        GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: _toggleAnimation,
          child: _currentFrame != null
              ? RawImage(
                  image: _currentFrame,
                  fit: widget.fit,
                  alignment: Alignment.center,
                )
              : const SizedBox.expand(),
        ),
        AttachmentImageOverlay(
          attachmentId: widget.attachment.attachmentId,
          size: widget.attachment.size,
          isSender: widget.isSender,
          isAnimationPaused: _stopped && _error == null,
          // A known classification implies the content is on device, so a
          // download can never be offered here.
          onTapDownload: () {},
        ),
      ],
    );
  }
}
