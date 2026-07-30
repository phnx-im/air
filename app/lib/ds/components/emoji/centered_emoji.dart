// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

/// Renders a single emoji glyph visually centered within its own layout box.
///
/// A raw [Text] (and plain layout-box centering) only centers the glyph's
/// layout box. That is enough on macOS, but on iOS the Apple Color Emoji glyph
/// is not centered within its own layout box (even `leadingDistribution: .even`
/// only evens the extra leading, not the font's asymmetric ascent/descent). To
/// fix it we measure the glyph's ink (visual) bounding box once, cache the
/// correction, and paint the glyph so its ink center lands at the center of the
/// paint box.
///
/// Measuring ink requires rasterizing the glyph, which is async, so the first
/// frame falls back to layout-box centering and snaps into place once the
/// correction is known. Corrections are cached process-wide; call [warmUp]
/// ahead of time to avoid the snap for a known set of emojis.
///
/// Laid-out glyph painters are cached process-wide as well and kept for the
/// lifetime of the app: measured cost is ~1 KiB per glyph, about ~2 MB for the
/// full picker set and ~4 MB with all skin tones warmed.
class CenteredEmoji extends StatefulWidget {
  const CenteredEmoji({super.key, required this.emoji, required this.style});

  final String emoji;
  final TextStyle style;

  /// Number of glyphs shaped per idle-priority chunk in [warmUpGlyphs].
  static const _warmUpChunkSize = 50;

  /// Laid-out glyph painters, keyed by [_glyphKey].
  static final Map<String, TextPainter> _glyphs = {};

  /// Correction (logical px) to add so the glyph's ink center sits at the
  /// box center. Keyed by [_correctionKey].
  static final Map<String, Offset> _corrections = {};
  static final Map<String, Future<Offset>> _inFlight = {};

  /// Resolves the effective style the same way [Text] does: a bare
  /// [TextPainter] would fall back to the engine default font family, not
  /// the theme's.
  static TextStyle _resolve(BuildContext context, TextStyle style) =>
      DefaultTextStyle.of(context).style.merge(style);

  static double _scaledSize(TextStyle style, TextScaler scaler) =>
      scaler.scale(style.fontSize ?? kDefaultFontSize);

  // The painter depends on the color (monochrome emoji fonts), the
  // correction only on the alpha channel; both depend on the font and the
  // effective glyph size.
  static String _glyphKey(String emoji, TextStyle style, double scaledSize) =>
      '$emoji|$scaledSize|${style.fontFamily}|${style.color}';
  static String _correctionKey(
    String emoji,
    TextStyle style,
    double scaledSize,
    double dpr,
  ) => '$emoji|$scaledSize|${style.fontFamily}|$dpr';

  /// The laid-out painter for [emoji], shaping and caching it on first use.
  ///
  /// The painter is shaped at the scaled font size, so callers with
  /// different (font size, text scaler) pairs share an entry when the
  /// effective glyph size is the same.
  static TextPainter _glyphOf(
    String emoji,
    TextStyle style,
    double scaledSize,
  ) {
    final key = _glyphKey(emoji, style, scaledSize);
    return _glyphs.putIfAbsent(
      key,
      () => TextPainter(
        text: TextSpan(
          text: emoji,
          style: style.copyWith(fontSize: scaledSize),
        ),
        textDirection: TextDirection.ltr,
        textHeightBehavior: const TextHeightBehavior(
          leadingDistribution: .even,
        ),
      )..layout(),
    );
  }

  /// Starts the ink measurement for [emoji] unless it is already cached or
  /// running. Returns the in-flight future, or `null` when already cached.
  static Future<Offset>? _ensureCorrection(
    String emoji,
    TextStyle style,
    double scaledSize,
    double dpr,
  ) {
    final key = _correctionKey(emoji, style, scaledSize, dpr);
    if (_corrections.containsKey(key)) {
      return null;
    }
    return _inFlight.putIfAbsent(key, () {
      final glyph = _glyphOf(emoji, style, scaledSize);
      final future = _measureInkCorrection(glyph, dpr);
      future.then((offset) {
        _corrections[key] = offset;
        _inFlight.remove(key);
      });
      return future;
    });
  }

  /// Shapes the glyphs and starts the ink measurements for [emojis], so they
  /// paint ink-centered on first frame instead of snapping into place.
  ///
  /// Idempotent and cheap when already cached. Meant for small sets (the
  /// quick-reaction bar); use [warmUpGlyphs] for the full picker set.
  static void warmUp(
    BuildContext context,
    Iterable<String> emojis,
    TextStyle style,
  ) {
    final resolved = _resolve(context, style);
    final scaledSize = _scaledSize(resolved, MediaQuery.textScalerOf(context));
    final dpr = MediaQuery.devicePixelRatioOf(context);
    for (final emoji in emojis) {
      _ensureCorrection(emoji, resolved, scaledSize, dpr);
    }
  }

  /// Shapes the glyph painters for [emojis] in idle-priority chunks, so a
  /// large grid doesn't shape every glyph on first paint.
  ///
  /// Does not measure ink corrections: rasterizing the full picker set would
  /// be far too expensive, so mounted widgets measure lazily instead.
  static void warmUpGlyphs(
    BuildContext context,
    List<String> emojis,
    TextStyle style,
  ) {
    final resolved = _resolve(context, style);
    final scaledSize = _scaledSize(resolved, MediaQuery.textScalerOf(context));

    var index = 0;
    void chunk() {
      final end = math.min(index + _warmUpChunkSize, emojis.length);
      for (; index < end; index++) {
        _glyphOf(emojis[index], resolved, scaledSize);
      }
      if (index < emojis.length) {
        SchedulerBinding.instance.scheduleTask(chunk, Priority.idle);
      }
    }

    SchedulerBinding.instance.scheduleTask(chunk, Priority.idle);
  }

  /// Completes when all currently running ink measurements are done.
  @visibleForTesting
  static Future<void> debugFlushMeasurements() =>
      Future.wait(_inFlight.values.toList());

  @visibleForTesting
  static void debugResetCaches() {
    _glyphs.clear();
    _corrections.clear();
    _inFlight.clear();
  }

  @override
  State<CenteredEmoji> createState() => _CenteredEmojiState();
}

class _CenteredEmojiState extends State<CenteredEmoji> {
  Offset _correction = Offset.zero;
  String? _key;

  @override
  Widget build(BuildContext context) {
    final scaler = MediaQuery.textScalerOf(context);
    final dpr = MediaQuery.devicePixelRatioOf(context);
    final style = CenteredEmoji._resolve(context, widget.style);
    final scaledSize = CenteredEmoji._scaledSize(style, scaler);
    final glyph = CenteredEmoji._glyphOf(widget.emoji, style, scaledSize);

    final key = CenteredEmoji._correctionKey(
      widget.emoji,
      style,
      scaledSize,
      dpr,
    );
    if (key != _key) {
      _key = key;
      final cached = CenteredEmoji._corrections[key];
      if (cached != null) {
        _correction = cached;
      } else {
        _correction = Offset.zero;
        // Measurements are shared per key, so every widget showing this
        // emoji is notified, even if several mount before the raster
        // completes.
        CenteredEmoji._ensureCorrection(
          widget.emoji,
          style,
          scaledSize,
          dpr,
        )?.then((offset) {
          if (mounted && _key == key) setState(() => _correction = offset);
        });
      }
    }

    // A raw Text would expose its content to the semantics tree, painting
    // the glyph ourselves loses that, so restore the label explicitly.
    return Semantics(
      label: widget.emoji,
      child: CustomPaint(
        size: Size(glyph.width, glyph.height),
        painter: CenteredGlyphPainter(glyph, _correction),
      ),
    );
  }
}

/// Rasterizes the laid-out [glyph] and returns the offset that moves its ink
/// (visual) center onto its layout-box center.
Future<Offset> _measureInkCorrection(TextPainter glyph, double dpr) async {
  final width = glyph.width;
  final height = glyph.height;
  final pw = (width * dpr).ceil();
  final ph = (height * dpr).ceil();
  if (pw <= 0 || ph <= 0) return Offset.zero;

  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder);
  canvas.scale(dpr);
  glyph.paint(canvas, Offset.zero);
  final picture = recorder.endRecording();
  final image = await picture.toImage(pw, ph);
  picture.dispose();
  final data = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  image.dispose();
  if (data == null) return Offset.zero;

  // Scan the alpha channel for the glyph's ink bounding box.
  final bytes = data.buffer.asUint8List();
  var minX = pw, minY = ph, maxX = -1, maxY = -1;
  for (var y = 0; y < ph; y++) {
    final rowOffset = y * pw * 4;
    for (var x = 0; x < pw; x++) {
      if (bytes[rowOffset + x * 4 + 3] != 0) {
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
    }
  }
  if (maxX < minX || maxY < minY) return Offset.zero;

  // Ink center in logical px, then the correction toward the layout center.
  final inkCenterX = (minX + maxX + 1) / 2 / dpr;
  final inkCenterY = (minY + maxY + 1) / 2 / dpr;
  return Offset(width / 2 - inkCenterX, height / 2 - inkCenterY);
}

/// Paints [glyph] centered within the paint box, plus a [correction] that
/// centers its ink rather than its layout box.
class CenteredGlyphPainter extends CustomPainter {
  const CenteredGlyphPainter(this.glyph, this.correction);

  final TextPainter glyph;
  final Offset correction;

  @override
  void paint(Canvas canvas, Size size) {
    glyph.paint(
      canvas,
      Offset(
        (size.width - glyph.width) / 2 + correction.dx,
        (size.height - glyph.height) / 2 + correction.dy,
      ),
    );
  }

  @override
  bool shouldRepaint(CenteredGlyphPainter oldDelegate) =>
      glyph != oldDelegate.glyph || correction != oldDelegate.correction;
}
