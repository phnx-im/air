// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/fullscreen_image/fullscreen_image_tokens.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:photo_view/photo_view.dart';
import 'package:photo_view/photo_view_gallery.dart';

/// The scale a page sits at unzoomed. We lay pages out at the size their
/// picture takes on screen, so the fit is a scale of exactly one rather than
/// something only the decode could tell us.
const double _fitScale = 1.0;

/// How far a scale may drift from [_fitScale] and still count as unzoomed.
/// Pinch never settles exactly, so an exact compare would leave the
/// drag-to-dismiss disabled after the smallest gesture.
const double _fitScaleTolerance = 0.02;

/// Type step of the gallery counter. Shared with the inset that has to keep the
/// picture clear of it, so the two can't drift apart.
TypeStyleToken get _counterType => typeScale.body.s;

/// One picture in the takeover.
@immutable
class FullscreenImageItem {
  const FullscreenImageItem({
    required this.image,
    required this.naturalSize,
    this.placeholder,
    this.heroTag,
  });

  final ImageProvider image;

  /// The picture's own pixel size. We lay the page out from it rather than
  /// from the decode, so the frame is in place before the picture arrives.
  final Size naturalSize;

  /// A cheaper decode of the same picture, painted under it. Only one already
  /// in the image cache is worth passing: the point is a frame that paints on
  /// the very first build, which is what the hero flight has to fly.
  final ImageProvider? placeholder;

  /// Ties this picture to the thumbnail it opened from. Null skips the flight.
  final Object? heroTag;
}

/// The pictures on their own, over a dark backdrop: pinch and double-tap to
/// zoom, scroll to zoom with a pointer, drag down to dismiss with a thumb, and
/// a header that gets out of the way on a tap so nothing sits over the picture.
///
/// A second picture brings out the gallery chrome -- arrows, a counter, and the
/// swipe and arrow keys between them. One picture renders none of it.
///
/// A pure view: it renders the pictures it's handed and reports the close
/// back, so the host owns the route and the transition into it.
class FullscreenImage extends StatefulWidget {
  const FullscreenImage({
    super.key,
    required this.tokens,
    required this.items,
    this.initialIndex = 0,
    required this.onClose,
    this.onShare,
    this.error,
  });

  final FullscreenImageTokens tokens;
  final List<FullscreenImageItem> items;

  /// Which picture opens. The rest are reachable from it.
  final int initialIndex;

  /// Replaces the picture when it fails to decode. Defaults to a broken-image
  /// glyph.
  final Widget? error;

  final VoidCallback onClose;

  /// Hands the picture on show to the platform. Null leaves the affordance out,
  /// as does a token set that keeps sharing off the header.
  final VoidCallback? onShare;

  @override
  State<FullscreenImage> createState() => _FullscreenImageState();
}

class _FullscreenImageState extends State<FullscreenImage> {
  late final PageController _pageController;

  /// One pair of zoom controllers per page, kept past the swipe away so a
  /// picture is still where it was left on the way back.
  final Map<int, _PageZoom> _zooms = {};

  StreamSubscription<PhotoViewControllerValue>? _scaleSubscription;
  Timer? _chromeTapTimer;

  late int _index;
  bool _chromeVisible = true;
  double _dragOffset = 0;
  bool _atBaseScale = true;

  bool get _canDrag => widget.tokens.dragToDismiss && _atBaseScale;
  bool get _hasGallery => widget.items.length > 1;

  @override
  void initState() {
    super.initState();
    _index = widget.initialIndex;
    _pageController = PageController(initialPage: widget.initialIndex);
    _listenToScale();
  }

  @override
  void dispose() {
    _chromeTapTimer?.cancel();
    unawaited(_scaleSubscription?.cancel());
    _pageController.dispose();
    for (final zoom in _zooms.values) {
      zoom.dispose();
    }
    super.dispose();
  }

  _PageZoom _zoomFor(int index) => _zooms.putIfAbsent(index, _PageZoom.new);

  /// Follows the page on show. Only its scale says anything about what the
  /// scroll wheel and the drag are allowed to do.
  void _listenToScale() {
    unawaited(_scaleSubscription?.cancel());
    final controller = _zoomFor(_index).controller;
    _scaleSubscription = controller.outputStateStream.listen(_handleScale);
    // The stream doesn't replay, and photo_view keeps some of its own moves
    // off it entirely, so a page may well be at a scale this never saw. Seed
    // from where the controller already is.
    _handleScale(controller.value);
  }

  void _handleScale(PhotoViewControllerValue value) {
    final scale = value.scale;
    if (scale == null) return;
    final zoom = _zoomFor(_index);
    zoom.currentScale = scale;
    final atBase = zoom.isAtBase;
    if (_atBaseScale == atBase) return;
    setState(() {
      _atBaseScale = atBase;
      // A zoomed picture pans instead of dismissing, so we abandon any drag in
      // flight rather than leave it half applied.
      if (!atBase) _dragOffset = 0;
    });
  }

  void _handlePageChanged(int index) {
    setState(() {
      _index = index;
      // The page arriving brings its own zoom, so the drag gate follows it
      // rather than staying on whatever the page leaving was at.
      _atBaseScale = _zoomFor(index).isAtBase;
      _dragOffset = 0;
    });
    _listenToScale();
  }

  /// Steps [step] pictures along, stopping at either end.
  void _page(int step) {
    final target = _index + step;
    if (target < 0 || target >= widget.items.length) return;
    unawaited(
      _pageController.animateToPage(
        target,
        duration: Effect.duration(FullscreenImageTokens.chromeMotion),
        curve: Effect.easeOutQuart,
      ),
    );
  }

  void _toggleChrome() => setState(() => _chromeVisible = !_chromeVisible);

  /// A tap toggles the chrome, but only once the double-tap window has passed:
  /// the second tap of a zoom would otherwise hide the header on the way in.
  void _handleTap() {
    final pending = _chromeTapTimer;
    if (pending != null && pending.isActive) {
      pending.cancel();
      _chromeTapTimer = null;
      return;
    }
    _chromeTapTimer = Timer(FullscreenImageTokens.chromeTapDelay, () {
      _chromeTapTimer = null;
      _toggleChrome();
    });
  }

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      widget.onClose();
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.space) {
      _toggleChrome();
      return KeyEventResult.handled;
    }
    final back = event.logicalKey == LogicalKeyboardKey.arrowLeft;
    if (back || event.logicalKey == LogicalKeyboardKey.arrowRight) {
      // A single picture has nowhere to step, so the arrows aren't ours to
      // swallow.
      if (!_hasGallery) return KeyEventResult.ignored;
      _page(back ? -1 : 1);
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  void _handlePointerSignal(PointerSignalEvent event) {
    if (event is! PointerScrollEvent) return;
    final zoom = _zoomFor(_index);
    final direction = event.scrollDelta.dy < 0 ? 1 : -1;
    final step = 1 + FullscreenImageTokens.scrollZoomStep * direction;
    zoom.controller.scale = (zoom.currentScale * step)
        .clamp(_fitScale, _fitScale * FullscreenImageTokens.maxZoomScale)
        .toDouble();
  }

  void _handleDragUpdate(DragUpdateDetails details) {
    final delta = details.primaryDelta ?? 0;
    // Only downward drags travel: an upward one just unwinds what's there.
    if (delta < 0 && _dragOffset <= 0) {
      _resetDrag();
      return;
    }
    setState(() => _dragOffset = math.max(0, _dragOffset + delta));
  }

  void _handleDragEnd(DragEndDetails details) {
    if (_dragOffset > FullscreenImageTokens.dismissThreshold) {
      widget.onClose();
      return;
    }
    _resetDrag();
  }

  void _resetDrag() {
    if (_dragOffset == 0) return;
    setState(() => _dragOffset = 0);
  }

  @override
  Widget build(BuildContext context) {
    final t = widget.tokens;
    // A permanent dark takeover: the chrome reads against the backdrop rather
    // than the app's surface, so it takes the dark palette in either theme.
    final palette = darkSemanticPalette;

    final dragging = _canDrag;
    final backdropOpacity = dragging
        ? (1 - _dragOffset / FullscreenImageTokens.dismissFadeDistance).clamp(
            0.0,
            1.0,
          )
        : 1.0;
    final pictureScale = dragging
        ? math.max(
            FullscreenImageTokens.dismissMinScale,
            1 - _dragOffset / FullscreenImageTokens.dismissScaleDistance,
          )
        : 1.0;

    return Focus(
      autofocus: true,
      onKeyEvent: _handleKey,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onVerticalDragUpdate: dragging ? _handleDragUpdate : null,
        onVerticalDragEnd: dragging ? _handleDragEnd : null,
        onVerticalDragCancel: dragging ? _resetDrag : null,
        child: Stack(
          fit: StackFit.expand,
          children: [
            _Backdrop(
              tokens: t,
              color: t.frostedBackdrop
                  ? palette.function.neutral.scrimDark
                  : palette.function.neutral.black,
              opacity: backdropOpacity,
            ),
            Transform(
              alignment: Alignment.center,
              transform: Matrix4.translationValues(
                0,
                dragging ? _dragOffset : 0,
                0,
              ).scaledByDouble(pictureScale, pictureScale, 1, 1),
              child: _Gallery(
                tokens: t,
                items: widget.items,
                pageController: _pageController,
                zoomFor: _zoomFor,
                error: widget.error,
                onTap: _handleTap,
                onPointerSignal: _handlePointerSignal,
                onPageChanged: _handlePageChanged,
              ),
            ),
            _Header(
              tokens: t,
              visible: _chromeVisible,
              opacity: backdropOpacity,
              onClose: widget.onClose,
              onShare: widget.onShare,
            ),
            if (_hasGallery) ...[
              _NavArrows(
                tokens: t,
                visible: _chromeVisible,
                opacity: backdropOpacity,
                canGoBack: _index > 0,
                canGoForward: _index < widget.items.length - 1,
                onBack: () => _page(-1),
                onForward: () => _page(1),
              ),
              _Counter(
                tokens: t,
                visible: _chromeVisible,
                opacity: backdropOpacity,
                index: _index,
                count: widget.items.length,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// The zoom of one page. photo_view splits it across two controllers, and a
/// page has to hold both for as long as it may come back into view.
class _PageZoom {
  _PageZoom()
    : controller = PhotoViewController(),
      scaleState = PhotoViewScaleStateController();

  final PhotoViewController controller;
  final PhotoViewScaleStateController scaleState;

  /// Where the page is now, as a multiple of its fit. It's what turns a
  /// pointer scroll into a zoom bounded by that fit, and it belongs to the page
  /// rather than the viewer: a page left zoomed is still zoomed when it's
  /// swiped back to.
  double currentScale = _fitScale;

  /// Whether the page sits at its fit, with nothing zoomed to pan.
  bool get isAtBase => (currentScale - _fitScale).abs() < _fitScaleTolerance;

  void dispose() {
    controller.dispose();
    scaleState.dispose();
  }
}

/// The dark plate the pictures sit on. Only the tint's alpha follows the drag:
/// animating the blur radius would re-blur the whole viewport every frame.
class _Backdrop extends StatelessWidget {
  const _Backdrop({
    required this.tokens,
    required this.color,
    required this.opacity,
  });

  final FullscreenImageTokens tokens;
  final Color color;
  final double opacity;

  @override
  Widget build(BuildContext context) {
    final tint = ColoredBox(color: color.withValues(alpha: color.a * opacity));
    if (!tokens.frostedBackdrop) return tint;
    final blur = Effect.blur(FullscreenImageTokens.backdropBlur);
    return BackdropFilter(
      filter: ImageFilter.blur(sigmaX: blur, sigmaY: blur),
      child: tint,
    );
  }
}

/// The pictures themselves, one per page, with the zoom and pan gestures on
/// them. The paging is photo_view's own: a hand-rolled horizontal drag would
/// fight the pan of a zoomed picture in the gesture arena, which photo_view
/// settles from the inside.
class _Gallery extends StatelessWidget {
  const _Gallery({
    required this.tokens,
    required this.items,
    required this.pageController,
    required this.zoomFor,
    required this.error,
    required this.onTap,
    required this.onPointerSignal,
    required this.onPageChanged,
  });

  final FullscreenImageTokens tokens;
  final List<FullscreenImageItem> items;
  final PageController pageController;
  final _PageZoom Function(int index) zoomFor;
  final Widget? error;
  final VoidCallback onTap;
  final ValueChanged<PointerSignalEvent> onPointerSignal;
  final ValueChanged<int> onPageChanged;

  /// Reserve at the bottom that keeps a picture clear of the counter. Without a
  /// gallery there's no counter to clear.
  EdgeInsets get _padding => items.length > 1
      ? EdgeInsets.fromLTRB(
          FullscreenImageTokens.imagePadding,
          FullscreenImageTokens.imagePadding,
          FullscreenImageTokens.imagePadding,
          FullscreenImageTokens.counterBottom +
              _counterType.lineHeightPx +
              S.s16,
        )
      : const EdgeInsets.all(FullscreenImageTokens.imagePadding);

  @override
  Widget build(BuildContext context) {
    return ClipRect(
      child: Listener(
        onPointerSignal: onPointerSignal,
        child: Padding(
          padding: _padding,
          child: LayoutBuilder(
            builder: (context, constraints) => PhotoViewGallery.builder(
              itemCount: items.length,
              builder: (context, index) =>
                  _pageOptions(index, constraints.biggest),
              pageController: pageController,
              onPageChanged: onPageChanged,
              // The backdrop below is the plate, so the gallery adds no fill of
              // its own.
              backgroundDecoration: const BoxDecoration(
                color: Color(0x00000000),
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// A page laid out from the size the caller declared rather than from the
  /// decode. photo_view's image branch mounts nothing -- hero included --
  /// until the picture has arrived, and a route push goes looking for the
  /// hero one frame in, so a picture still on its way would open with no
  /// flight at all.
  PhotoViewGalleryPageOptions _pageOptions(int index, Size viewport) {
    final item = items[index];
    final tag = item.heroTag;
    final zoom = zoomFor(index);

    return PhotoViewGalleryPageOptions.customChild(
      // The frame is the picture at its fit, so a scale of 1 is the fit and
      // whatever is drawn inside it is drawn at the size it was written for.
      childSize: applyBoxFit(
        BoxFit.contain,
        item.naturalSize,
        viewport,
      ).destination,
      child: _Picture(tokens: tokens, item: item, error: error),
      // No filterQuality on purpose: naming one has photo_view lay the picture
      // out anew at every scale, which only its image branch can do. The zoom
      // rides the transform around the frame instead, and the pictures in it
      // carry their own quality.
      controller: zoom.controller,
      scaleStateController: zoom.scaleState,
      heroAttributes: tag == null
          ? null
          : PhotoViewHeroAttributes(tag: tag, transitionOnUserGestures: true),
      minScale: PhotoViewComputedScale.contained,
      maxScale:
          PhotoViewComputedScale.covered * FullscreenImageTokens.maxZoomScale,
      scaleStateCycle: _doubleTapScaleStateCycle,
      onTapUp: (context, details, value) => onTap(),
    );
  }
}

/// The picture on one page, over the stand-in that holds its frame until it
/// arrives. Both fill the frame the page laid out, so the sharp decode lands
/// exactly where the stand-in was.
class _Picture extends StatelessWidget {
  const _Picture({
    required this.tokens,
    required this.item,
    required this.error,
  });

  final FullscreenImageTokens tokens;
  final FullscreenImageItem item;
  final Widget? error;

  @override
  Widget build(BuildContext context) {
    final palette = darkSemanticPalette;
    final placeholder = item.placeholder;

    return Stack(
      fit: StackFit.expand,
      children: [
        if (placeholder != null)
          Image(
            image: placeholder,
            fit: BoxFit.contain,
            filterQuality: FilterQuality.medium,
          ),
        Image(
          image: item.image,
          fit: BoxFit.contain,
          filterQuality: FilterQuality.medium,
          // A stand-in already holds the frame, so the transfer runs behind the
          // picture rather than behind a spinner.
          loadingBuilder: (context, child, event) =>
              event == null || placeholder != null
              ? child
              : _Loading(
                  color: palette.text.primary,
                  loaded: event.cumulativeBytesLoaded,
                  total: event.expectedTotalBytes,
                ),
          errorBuilder: (context, exception, stackTrace) => Center(
            child:
                error ??
                AppIcon(
                  type: AppIconType.imageOff,
                  size: FullscreenImageTokens.errorIconSize,
                  color: palette.text.quaternary,
                ),
          ),
        ),
      ],
    );
  }
}

/// Transfer progress while the picture is still arriving. Determinate once the
/// size is known, so a slow attachment shows how far along it is.
class _Loading extends StatelessWidget {
  const _Loading({
    required this.color,
    required this.loaded,
    required this.total,
  });

  final Color color;
  final int loaded;
  final int? total;

  @override
  Widget build(BuildContext context) {
    final total = this.total;
    return Center(
      child: CircularProgressIndicator(
        valueColor: AlwaysStoppedAnimation<Color>(color),
        value: total == null ? null : loaded / total,
      ),
    );
  }
}

/// One layer of chrome. Every layer fades and stops taking taps together, so a
/// tap that hides the chrome leaves nothing at all over the picture.
class _Chrome extends StatelessWidget {
  const _Chrome({
    required this.tokens,
    required this.visible,
    required this.opacity,
    required this.child,
  });

  final FullscreenImageTokens tokens;
  final bool visible;
  final double opacity;
  final Widget child;

  @override
  Widget build(BuildContext context) => IgnorePointer(
    ignoring: !visible,
    child: AnimatedOpacity(
      duration: Effect.duration(FullscreenImageTokens.chromeMotion),
      curve: Effect.easeOutQuart,
      opacity: visible ? opacity : 0,
      child: child,
    ),
  );
}

/// The close button, and the share opposite it where a thumb wants one. No fill
/// behind them: each button carries its own surround, so the takeover reads as
/// one unbroken dark field.
class _Header extends StatelessWidget {
  const _Header({
    required this.tokens,
    required this.visible,
    required this.opacity,
    required this.onClose,
    required this.onShare,
  });

  final FullscreenImageTokens tokens;
  final bool visible;
  final double opacity;
  final VoidCallback onClose;
  final VoidCallback? onShare;

  @override
  Widget build(BuildContext context) {
    final white = darkSemanticPalette.function.neutral.white;
    final share = onShare;
    final close = _button(AppIconType.x, white, onClose);

    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: _Chrome(
        tokens: tokens,
        visible: visible,
        opacity: opacity,
        child: SafeArea(
          bottom: false,
          child: ConstrainedBox(
            // A strip on a phone, and on a pointer a button floating in the
            // corner, where the padding alone is taller than the strip.
            constraints: const BoxConstraints(
              minHeight: FullscreenImageTokens.headerHeight,
            ),
            child: Padding(
              padding: tokens.headerPadding,
              child: Row(
                children: [
                  if (!tokens.closeOnTrailingEdge) close,
                  const Spacer(),
                  if (tokens.closeOnTrailingEdge) close,
                  if (tokens.shareInHeader && share != null)
                    _button(AppIconType.share, white, share),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _button(AppIconType icon, Color color, VoidCallback onPressed) =>
      ButtonIcon(
        variant: FullscreenImageTokens.buttonVariant,
        size: FullscreenImageTokens.buttonSize,
        icon: icon,
        iconSize: FullscreenImageTokens.buttonIconSize,
        iconColor: color,
        onPressed: onPressed,
      );
}

/// The step-through arrows, one at each edge. An arrow with nowhere left to go
/// fades out where it stands rather than leaving the row, so the one still live
/// never shifts under the pointer.
class _NavArrows extends StatelessWidget {
  const _NavArrows({
    required this.tokens,
    required this.visible,
    required this.opacity,
    required this.canGoBack,
    required this.canGoForward,
    required this.onBack,
    required this.onForward,
  });

  final FullscreenImageTokens tokens;
  final bool visible;
  final double opacity;
  final bool canGoBack;
  final bool canGoForward;
  final VoidCallback onBack;
  final VoidCallback onForward;

  @override
  Widget build(BuildContext context) {
    return Positioned.fill(
      child: _Chrome(
        tokens: tokens,
        visible: visible,
        opacity: opacity,
        // Only the two circles take taps: the row and the gap between them let
        // a tap through to the picture.
        child: Row(
          children: [
            Padding(
              padding: const EdgeInsets.only(
                left: FullscreenImageTokens.navEdgePadding,
              ),
              child: _arrow(AppIconType.chevronLeft, canGoBack, onBack),
            ),
            const Spacer(),
            Padding(
              padding: const EdgeInsets.only(
                right: FullscreenImageTokens.navEdgePadding,
              ),
              child: _arrow(AppIconType.chevronRight, canGoForward, onForward),
            ),
          ],
        ),
      ),
    );
  }

  /// An arrow at the end of the run reads spent twice over: the fade takes it
  /// off the picture, and the missing handler leaves [ButtonIcon] disabled, so
  /// it neither invites a press nor swallows one.
  Widget _arrow(AppIconType icon, bool enabled, VoidCallback onPressed) =>
      AnimatedOpacity(
        duration: Effect.duration(FullscreenImageTokens.navFadeMotion),
        curve: Effect.easeOutQuart,
        opacity: enabled ? FullscreenImageTokens.navIdleOpacity : 0,
        child: ButtonIcon(
          variant: FullscreenImageTokens.buttonVariant,
          size: FullscreenImageTokens.navButtonSize,
          icon: icon,
          iconSize: FullscreenImageTokens.navIconSize,
          iconColor: darkSemanticPalette.function.neutral.white,
          onPressed: enabled ? onPressed : null,
        ),
      );
}

/// How far into the run of pictures we are.
class _Counter extends StatelessWidget {
  const _Counter({
    required this.tokens,
    required this.visible,
    required this.opacity,
    required this.index,
    required this.count,
  });

  final FullscreenImageTokens tokens;
  final bool visible;
  final double opacity;
  final int index;
  final int count;

  @override
  Widget build(BuildContext context) {
    return Positioned(
      bottom: FullscreenImageTokens.counterBottom,
      left: 0,
      right: 0,
      child: _Chrome(
        tokens: tokens,
        visible: visible,
        opacity: opacity,
        child: Center(
          child: Text(
            '${index + 1} / $count',
            style: _counterType.style(color: darkSemanticPalette.text.primary),
          ),
        ),
      ),
    );
  }
}

/// A double tap fills the viewport, the next one returns to the fit.
PhotoViewScaleState _doubleTapScaleStateCycle(PhotoViewScaleState actual) =>
    switch (actual) {
      PhotoViewScaleState.initial ||
      PhotoViewScaleState.zoomedOut => PhotoViewScaleState.covering,
      PhotoViewScaleState.covering ||
      PhotoViewScaleState.zoomedIn ||
      PhotoViewScaleState.originalSize => PhotoViewScaleState.initial,
    };
