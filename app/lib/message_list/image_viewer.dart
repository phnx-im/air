// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:math' as math;

import 'package:air/attachments/attachment_image_provider.dart';
import 'package:air/core/core.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/theme/responsive_screen.dart';
import 'package:air/widgets/app_bar_back_button.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:photo_view/photo_view.dart';

String imageViewerHeroTag(UiAttachment attachment) =>
    'image-viewer-${attachment.attachmentId.uuid}';

Route<void> imageViewerRoute({required UiAttachment attachment}) {
  return PageRouteBuilder<void>(
    transitionDuration: const Duration(milliseconds: 280),
    reverseTransitionDuration: const Duration(milliseconds: 220),
    pageBuilder: (context, animation, secondaryAnimation) {
      return ImageViewer(attachment: attachment);
    },
    transitionsBuilder: (context, animation, secondaryAnimation, child) {
      final fadeAnimation = animation.drive(
        CurveTween(curve: Curves.easeInOutCubicEmphasized),
      );
      return FadeTransition(opacity: fadeAnimation, child: child);
    },
  );
}

class ImageViewer extends HookWidget {
  const ImageViewer({required this.attachment, super.key});

  final UiAttachment attachment;

  @override
  Widget build(BuildContext context) {
    final appBarIsVisible = useState(true);
    final dragOffset = useState(0.0);
    final isAtBaseScale = useState(true);
    final initialScale = useRef<double?>(null);
    final pendingTapTimer = useRef<Timer?>(null);

    useEffect(
      () => () {
        pendingTapTimer.value?.cancel();
      },
      [],
    );

    final isDesktop = ResponsiveScreen.isDesktop(context);
    final enableVerticalDrag = !isDesktop && isAtBaseScale.value;
    final backgroundOpacity =
        isDesktop ? 1.0 : (1 - (dragOffset.value / 300)).clamp(0.0, 1.0);
    final verticalOffset = enableVerticalDrag ? dragOffset.value : 0.0;

    useEffect(() {
      if (!enableVerticalDrag && dragOffset.value != 0) {
        dragOffset.value = 0;
      }
      return null;
    }, [enableVerticalDrag]);

    void toggleChrome() {
      appBarIsVisible.value = !appBarIsVisible.value;
    }

    void handleTap() {
      final existing = pendingTapTimer.value;
      if (existing != null && existing.isActive) {
        existing.cancel();
        pendingTapTimer.value = null;
        return;
      }
      pendingTapTimer.value = Timer(const Duration(milliseconds: 250), () {
        toggleChrome();
        pendingTapTimer.value = null;
      });
    }

    void handleScaleChanged(double scale) {
      initialScale.value ??= scale;
      final baseScale = initialScale.value!;
      final bool atBase = (scale - baseScale).abs() < 0.02;
      if (isAtBaseScale.value != atBase) {
        isAtBaseScale.value = atBase;
        if (!atBase && dragOffset.value != 0) {
          dragOffset.value = 0;
        }
      }
    }

    void handleVerticalDragUpdate(DragUpdateDetails details) {
      if (!enableVerticalDrag) {
        return;
      }
      final delta = details.primaryDelta ?? 0;
      if (delta < 0 && dragOffset.value <= 0) {
        dragOffset.value = 0;
        return;
      }
      dragOffset.value = math.max(0, dragOffset.value + delta);
    }

    void handleVerticalDragEnd(DragEndDetails details) {
      if (!enableVerticalDrag) {
        return;
      }
      if (dragOffset.value > 120) {
        HapticFeedback.lightImpact();
        Navigator.pop(context);
        return;
      }
      dragOffset.value = 0;
    }

    return Scaffold(
      backgroundColor: Colors.black.withValues(alpha: backgroundOpacity),
      body: Focus(
        autofocus: true,
        onKeyEvent: (node, event) {
          if (event.logicalKey == LogicalKeyboardKey.escape &&
              event is KeyDownEvent) {
            Navigator.pop(context);
            return KeyEventResult.handled;
          }
          if (isDesktop &&
              event is KeyDownEvent &&
              event.logicalKey == LogicalKeyboardKey.space) {
            toggleChrome();
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onVerticalDragUpdate:
              enableVerticalDrag ? handleVerticalDragUpdate : null,
          onVerticalDragEnd: enableVerticalDrag ? handleVerticalDragEnd : null,
          onVerticalDragCancel:
              enableVerticalDrag ? () => dragOffset.value = 0 : null,
          child: Stack(
            children: [
              Transform.translate(
                offset: Offset(0, verticalOffset),
                child: _ZoomableImage(
                  attachment: attachment,
                  onTap: handleTap,
                  onScaleChanged: handleScaleChanged,
                ),
              ),
              _ViewerOverlay(
                isVisible: appBarIsVisible.value,
                fadeOpacity: backgroundOpacity,
                title: attachment.filename,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ZoomableImage extends HookWidget {
  const _ZoomableImage({
    required this.attachment,
    required this.onTap,
    required this.onScaleChanged,
  });

  final UiAttachment attachment;
  final VoidCallback onTap;
  final ValueChanged<double> onScaleChanged;

  @override
  Widget build(BuildContext context) {
    final photoViewController = useMemoized(PhotoViewController.new);
    final scaleStateController = useMemoized(PhotoViewScaleStateController.new);

    final colors = CustomColorScheme.of(context);

    useEffect(
      () => () {
        photoViewController.dispose();
      },
      [photoViewController],
    );

    useEffect(
      () => () {
        scaleStateController.dispose();
      },
      [scaleStateController],
    );

    useEffect(() {
      final subscription = photoViewController.outputStateStream.listen((
        value,
      ) {
        final scale = value.scale;
        if (scale != null) {
          onScaleChanged(scale);
        }
      });
      return subscription.cancel;
    }, [photoViewController, onScaleChanged]);

    return ClipRect(
      child: PhotoView(
        controller: photoViewController,
        scaleStateController: scaleStateController,
        heroAttributes: PhotoViewHeroAttributes(
          tag: imageViewerHeroTag(attachment),
          transitionOnUserGestures: true,
        ),
        backgroundDecoration: const BoxDecoration(color: Colors.transparent),
        minScale: PhotoViewComputedScale.contained,
        maxScale: PhotoViewComputedScale.covered * 4.0,
        scaleStateCycle: _doubleTapScaleStateCycle,
        filterQuality: FilterQuality.high,
        loadingBuilder: (context, event) {
          if (event == null) {
            return const SizedBox.shrink();
          }
          return Center(
            child: CircularProgressIndicator(
              valueColor: AlwaysStoppedAnimation<Color>(
                colors.backgroundBase.tertiary,
              ),
              value:
                  event.expectedTotalBytes != null
                      ? event.cumulativeBytesLoaded / event.expectedTotalBytes!
                      : null,
            ),
          );
        },
        errorBuilder:
            (context, error, stackTrace) => Center(
              child: Icon(
                Icons.broken_image_outlined,
                color: colors.text.primary,
                size: 48,
              ),
            ),
        imageProvider: AttachmentImageProvider(
          attachment: attachment,
          attachmentsRepository: RepositoryProvider.of(context),
        ),
        onTapUp: (context, details, value) => onTap(),
      ),
    );
  }
}

class _ViewerOverlay extends StatelessWidget {
  const _ViewerOverlay({
    required this.isVisible,
    required this.fadeOpacity,
    required this.title,
  });

  final bool isVisible;
  final double fadeOpacity;
  final String title;

  @override
  Widget build(BuildContext context) {
    final colorScheme = CustomColorScheme.of(context);
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: IgnorePointer(
        ignoring: !isVisible,
        child: AnimatedOpacity(
          duration: const Duration(milliseconds: 250),
          opacity: isVisible ? fadeOpacity : 0,
          child: Container(
            color: colorScheme.backgroundBase.primary.withValues(alpha: 0.7),
            child: AppBar(
              leading: const AppBarBackButton(),
              backgroundColor: Colors.transparent,
              elevation: 0,
              title: Text(title),
              centerTitle: true,
            ),
          ),
        ),
      ),
    );
  }
}

PhotoViewScaleState _doubleTapScaleStateCycle(PhotoViewScaleState actual) {
  switch (actual) {
    case PhotoViewScaleState.initial:
    case PhotoViewScaleState.zoomedOut:
      return PhotoViewScaleState.covering;
    case PhotoViewScaleState.covering:
    case PhotoViewScaleState.zoomedIn:
    case PhotoViewScaleState.originalSize:
      return PhotoViewScaleState.initial;
  }
}
