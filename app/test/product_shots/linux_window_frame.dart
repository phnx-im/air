// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The window product shot: the app in a GNOME window, the way a window
/// screenshot on the desktop captures it, with no marketing chrome around it.
///
/// Mirrors the GTK3 header bar the Linux runner sets up
/// (`linux/runner/my_application.cc`).
library;

import 'package:flutter/material.dart';

// Adwaita (GTK3) metrics.
const _headerBarHeight = 46.0;
const _cornerRadius = 8.0;
const _closeButtonSize = 24.0;
const _closeButtonMargin = 11.0;

/// Room around the window for its shadow.
const _shadowMargin = 32.0;

class LinuxWindowFrame extends StatelessWidget {
  const LinuxWindowFrame({
    super.key,
    required this.screenSize,
    required this.devicePixelRatio,
    required this.title,
    required this.child,
    this.scale = 1.0,
  });

  /// The logical content area the embedded app renders into, below the
  /// header bar.
  final Size screenSize;
  final double devicePixelRatio;
  final String title;
  final Widget child;

  /// How many output pixels to emit per logical pixel of the window.
  final double scale;

  /// The logical size of the frame, shadow margin included.
  static Size sizeFor(Size screenSize) => Size(
    screenSize.width + 2 * _shadowMargin,
    screenSize.height + _headerBarHeight + 2 * _shadowMargin,
  );

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = theme.brightness == Brightness.dark
        ? _HeaderBarPalette.dark
        : _HeaderBarPalette.light;
    final baseMediaQuery =
        MediaQuery.maybeOf(context) ?? const MediaQueryData();

    const radius = BorderRadius.vertical(top: Radius.circular(_cornerRadius));

    final window = Container(
      margin: const EdgeInsets.all(_shadowMargin),
      // The shadow and outline GTK3 draws around a focused CSD window.
      decoration: const BoxDecoration(
        borderRadius: radius,
        boxShadow: [
          BoxShadow(
            color: Color(0x80000000),
            offset: Offset(0, 3),
            blurRadius: 9,
            spreadRadius: 1,
          ),
          BoxShadow(color: Color(0x3B000000), spreadRadius: 1),
        ],
      ),
      child: ClipRRect(
        borderRadius: radius,
        clipBehavior: .antiAlias,
        child: Column(
          crossAxisAlignment: .stretch,
          children: [
            _HeaderBar(title: title, palette: palette),
            SizedBox.fromSize(
              size: screenSize,
              child: ColoredBox(
                color: theme.canvasColor,
                child: MediaQuery(
                  data: baseMediaQuery.copyWith(
                    size: screenSize,
                    devicePixelRatio: devicePixelRatio,
                    padding: EdgeInsets.zero,
                    viewPadding: EdgeInsets.zero,
                    viewInsets: EdgeInsets.zero,
                    systemGestureInsets: EdgeInsets.zero,
                  ),
                  child: child,
                ),
              ),
            ),
          ],
        ),
      ),
    );

    final size = sizeFor(screenSize);
    return RepaintBoundary(
      child: SizedBox.fromSize(
        size: size * scale,
        // Same aspect ratio either side, so `contain` scales by exactly
        // [scale] without letterboxing.
        child: FittedBox(
          fit: .contain,
          child: SizedBox.fromSize(size: size, child: window),
        ),
      ),
    );
  }
}

class _HeaderBar extends StatelessWidget {
  const _HeaderBar({required this.title, required this.palette});

  final String title;
  final _HeaderBarPalette palette;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: _headerBarHeight,
      decoration: BoxDecoration(
        color: palette.background,
        border: Border(bottom: BorderSide(color: palette.border)),
      ),
      child: Stack(
        children: [
          Center(
            child: Text(
              title,
              style: TextStyle(
                fontSize: 15,
                fontWeight: .w700,
                color: palette.foreground,
              ),
            ),
          ),
          Positioned(
            top: 0,
            bottom: 0,
            right: _closeButtonMargin,
            child: Center(child: _CloseButton(color: palette.foreground)),
          ),
        ],
      ),
    );
  }
}

/// GNOME's default button layout only has a close button.
class _CloseButton extends StatelessWidget {
  const _CloseButton({required this.color});

  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: _closeButtonSize,
      height: _closeButtonSize,
      decoration: BoxDecoration(
        shape: .circle,
        color: color.withValues(alpha: 0.1),
      ),
      child: CustomPaint(painter: _CrossPainter(color)),
    );
  }
}

/// The `window-close-symbolic` glyph, drawn so the shot needs no icon font.
class _CrossPainter extends CustomPainter {
  const _CrossPainter(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    const halfExtent = 4.0;
    final center = size.center(Offset.zero);
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.5
      ..strokeCap = .round;
    canvas
      ..drawLine(
        center + const Offset(-halfExtent, -halfExtent),
        center + const Offset(halfExtent, halfExtent),
        paint,
      )
      ..drawLine(
        center + const Offset(halfExtent, -halfExtent),
        center + const Offset(-halfExtent, halfExtent),
        paint,
      );
  }

  @override
  bool shouldRepaint(_CrossPainter oldDelegate) => color != oldDelegate.color;
}

class _HeaderBarPalette {
  const _HeaderBarPalette({
    required this.background,
    required this.border,
    required this.foreground,
  });

  final Color background;
  final Color border;
  final Color foreground;

  static const light = _HeaderBarPalette(
    background: Color(0xFFEBEBEB),
    border: Color(0xFFD6D6D6),
    foreground: Color(0xFF2E3436),
  );

  static const dark = _HeaderBarPalette(
    background: Color(0xFF303030),
    border: Color(0xFF1B1B1B),
    foreground: Color(0xFFEEEEEC),
  );
}
